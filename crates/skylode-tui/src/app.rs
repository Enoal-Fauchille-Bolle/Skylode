//! The application: state, the render loop, and the reducer that mutates it.
//!
//! `App` owns **UI state only** — which tab is open, which modal is stacked, the
//! live toasts. It deliberately owns no game rules: those belong to
//! `skylode-core`, and what the screens draw arrives as a flat [`View`] snapshot.
//! Keeping the split means a list cursor never leaks into a save file, and the
//! core stays testable without a terminal.

use std::time::{Duration, Instant};

use color_eyre::Result;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::Tabs,
};
use skylode_core::{
    economy::{self, Affordability, Shortfall},
    enchant::EnchantType,
    error::CoreError,
    game::{GameEvent, GameState, Input},
    material::{Item, Material},
    mine::Mine,
    mine_kind::MineKind,
    prestige,
    tunables::{LEVEL_CAP, RAW_PER_COMPRESSED, TICKS_PER_SECOND},
    upgrade,
};

use crate::{
    action::Action,
    announce,
    config::Config,
    cursor::{self, Cursors, MineTrack, UpgradeTab},
    flash::Flashes,
    format::{denominations, grouped, multiplier, prestige_rank, roman, rung_label, shown_rung},
    overlay::{
        Conversion, Modal, compression, dip, help,
        prestige::{self as prestige_overlay, CONFIRM_WORD},
        too_small,
    },
    screen::Screen,
    theme,
    toast::{TOAST_TTL, Toasts, Tone},
    view::{CompressHint, UpgradeDetail, View},
};

#[cfg(debug_assertions)]
use crate::format::grouped_u64;
#[cfg(debug_assertions)]
use crate::overlay::dev::{self, DevRow, DevState};
/// The dev menu's own imports, kept in a statement of their own.
///
/// A `#[cfg]` cannot be attached to one name inside a braced `use` tree, so gating these
/// means a second `use` rather than four lines in the blocks above. Gating them is not
/// tidiness: [`Paragraph`] and [`grouped_u64`](crate::format::grouped_u64) are reached
/// from dev code alone, so left in the main block they are two `unused_imports` warnings
/// in a release build — which `clippy -D warnings` never sees, because it builds the dev
/// profile. This is the one seam where that blind spot bites, and it is why
/// `cargo check --release` is on the verification list in `docs/DEV-MENU.md`.
#[cfg(debug_assertions)]
use ratatui::widgets::Paragraph;

/// The widest the interface is ever drawn, whatever the terminal offers.
///
/// **Twice the counted frame**, and that is the whole justification: the wireframes
/// in UI-EN.md §5 are 80 columns of *deliberately dense* text, so at 240 columns a
/// detail pane would be a hundred columns of whitespace with a forty-column
/// sentence adrift in it. Past this width the surplus becomes margin either side
/// rather than more line to cross with the eye.
///
/// `pub(crate)` because the title screen obeys the same cap without going through
/// [`App::render`] — [`splash`](crate::overlay::splash) is drawn straight from
/// [`Session`](crate::session::Session), and a title that spread to 240 columns
/// would put its version corner a screen's width from its own key hints.
pub(crate) const MAX_WIDTH: u16 = 2 * too_small::MIN_WIDTH;

/// The tallest, for the same reason and by the same arithmetic.
///
/// This one bites less often — a list genuinely uses every row it is given — but
/// the Mine screen's grid is a game constant, so a 90-row terminal would strand it
/// in the middle of an enormous empty box.
///
/// `pub(crate)` for [`MAX_WIDTH`]'s reason.
pub(crate) const MAX_HEIGHT: u16 = 2 * too_small::MIN_HEIGHT;

/// One simulation step, derived from the core's own tick rate.
///
/// **Derived and not written down**, because 20 tps is a game rule
/// (`docs/MECHANICS.md`) and a front-end that spelled `50` would be a second copy of
/// it — one that could be edited without the balance pass noticing. The division is
/// exact for any rate that divides a second evenly, and nanoseconds are what make it
/// exact at all: at 30 tps, milliseconds would floor to 33 and lose 1% of the day.
const SIM_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / TICKS_PER_SECOND);

/// How long after the mine key was last heard from the player still counts as
/// holding it (`docs/SYSTEMS.md`).
///
/// **1100 ms is not a preference.** A terminal that cannot report a release leaves
/// only OS auto-repeat to observe, and this window must outlast the longest *initial*
/// repeat delay a user setting can produce — Windows caps that at 1000 ms — or mining
/// would cut out in the gap between the first press and the second, hitching on every
/// hold. The cost is up to 1.1 s of over-mining after a release, which is invisible
/// against a seven-day offline cap; the alternative cost is a stutter the player feels
/// every single time.
const HOLD_WINDOW: Duration = Duration::from_millis(1_100);

/// The most simulation steps one [`App::advance`] will run before giving up and
/// resynchronising.
///
/// A second's worth. Without it, a laptop closed mid-session would hand the loop an
/// hour of arrears and replay seventy-two thousand ticks in one frame — freezing the
/// interface to compute what the offline accrual computes with one multiplication
/// (`GameState::resume`). Dropping the surplus is therefore not a rounding error but
/// a deferral to the mechanism that owns long absences.
const MAX_CATCHUP_TICKS: u32 = 20;

/// What the keyboard last said about the mine key, pending
/// [`App::advance`]'s reading of it.
///
/// **An edge, not a state.** The state — *is the player mining* — is a question about
/// time, and neither [`crate::keymap`] nor [`App::update`] may read a clock: one is a
/// pure decode, the other a pure reducer, and both are unit-tested on that basis. So
/// the reducer records only *what happened*, and `advance` stamps it with the instant
/// it already has in hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MineKeyEdge {
    /// Pressed, or auto-repeated, which the window treats identically.
    Down,
    /// Released — reportable only under the kitty keyboard protocol.
    Up,
}

/// How far a purchase on the Upgrades screen goes.
///
/// The only difference between `Enter` and `M`, named so that
/// [`App::buy_at_cursor`] can take it as an argument instead of existing twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// Up to the row the cursor is on — `Enter`.
    ToCursor,
    /// Up to whatever the inventory allows — `M`.
    AsFarAsPossible,
}

/// How a run ends, when the player has asked it to.
///
/// **One field of two values and not two booleans.** `q` and `Ctrl-C` are different
/// exits — one goes back to the title, the other ends the program — and a pair of
/// flags would make "leaving to the title *and* to the process" a state the type
/// allows and nothing forbids. It is the same answer this crate already gives for
/// [`dev`](App#structfield.dev) and for the running boost: when two things cannot
/// both be true, say so with the type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leaving {
    /// Back to the splash, with the run written first (`q`).
    ToTitle,
    /// Out of the program altogether (`Ctrl-C`).
    Process,
}

/// The whole front-end state.
#[derive(Debug)]
pub struct App {
    /// How the player has asked to leave, if they have.
    ///
    /// Set by the reducer and read by [`Session`](crate::session::Session), which owns
    /// what each answer *means*: the run is written either way, and only one of the two
    /// ends the loop. `App` deliberately does not act on it — a type that owns no
    /// disk and no terminal cannot be the one that decides what leaving costs.
    pub leaving: Option<Leaving>,
    /// The tab currently on screen.
    pub screen: Screen,
    /// The modal stacked over it, if any.
    ///
    /// **It carries the modal's own state, not just which one is up**, which is why
    /// [`Modal::Compress`] has fields: a dialog with a value in it has nowhere else to
    /// keep that value where "no dialog" and "a dialog reading zero" stay distinct.
    /// [`crate::keymap`] gives whatever is here first refusal on every key, and
    /// [`update`](App::update) gives it first refusal on every gesture.
    pub modal: Option<Modal>,
    /// Live announcements, drawn over everything.
    pub toasts: Toasts,
    /// Which cells a spatial blast has claimed, and when (UI.md §7).
    ///
    /// **The toast's sibling, and deliberately uncoupled from it.** Both are fed by the
    /// same [`GameEvent::SpatialProc`], and neither waits for the other: the toast says
    /// *what* (`Nuke — 200 blocks`) for three seconds, this says *where* for two hundred
    /// milliseconds, and the two windows have nothing to say to each other.
    ///
    /// It lives here for the reason the toasts do — it is a consequence of a past event
    /// rather than a fact about the run, so no `GameState` could answer it and no save
    /// should carry it.
    ///
    /// [`GameEvent::SpatialProc`]: skylode_core::game::GameEvent::SpatialProc
    pub flash: Flashes,
    /// The run itself — the rules, and every number the screens report.
    ///
    /// **`App` owns it rather than borrowing it**, because the run has no other
    /// home: `main` builds one and hands it over, and the tick mutates it from inside
    /// the loop. The boundary this crate keeps is not "the front-end
    /// may not hold game state" — it is that the front-end holds no game *rules*.
    /// Every field of `GameState` is private and every mutation goes through a
    /// method that can refuse.
    pub state: GameState,
    /// What the screens render — [`View::from_state`]'s answer, cached.
    ///
    /// Rebuilt by [`sync_view`](App::sync_view) before each draw rather than inside
    /// `render`, for two reasons. `render` takes `&self` and stays a pure read, which
    /// is what lets a test draw an `App` it does not own; and the projection still
    /// rebuilds `View::sample`'s fixture for the two screens phase 7 has not wired,
    /// which is worth doing when the state changes and not thirty times a second.
    pub view: View,
    /// Where the player is pointing on each list — front-end state, never the run's.
    ///
    /// Held beside [`state`](App::state) rather than inside it, because a highlighted
    /// row is not something a save should carry and not something the rules may
    /// consult. [`View::from_state`] reads both to build one snapshot.
    pub cursors: Cursors,
    /// The last purchase refused for a *denomination*, kept for the Inventory screen.
    ///
    /// **The §8.4 loop's only piece of memory.** A `CompressFirst` refusal sends the
    /// player to `3 Inventory` to convert by hand, and the panel there has to name what
    /// they came for — a screen that just said `Compressible now: 4` would have lost
    /// the question. The other half of that loop, *"the Upgrades selection is
    /// remembered"*, costs nothing: the cursors live here and not in the screens, so
    /// they survive the walk already.
    ///
    /// **Only the compress-first branch is kept.** `Insufficient` is answered by
    /// mining, not by anything on the Inventory screen, so remembering it would put a
    /// note on a screen that cannot act on it.
    pub refused: Option<CompressHint>,
    /// Front-end preferences — read while drawing, edited by Settings (phase 7).
    pub config: Config,
    /// The dev menu's state, or [`None`] when this session was not started with it.
    ///
    /// **One field carrying both layers of the gate**, which is what keeps the two from
    /// being able to disagree. The *outer* layer is `#[cfg(debug_assertions)]`: in a
    /// release build this field does not exist, so neither does the key that opens the
    /// menu, the modal it opens, or the branch that makes a purchase free. The *inner*
    /// layer is the [`Option`]: in a debug build the field exists and is `None` unless
    /// `main` found `SKYLODE_DEV` in the environment, so an ordinary `cargo run` is an
    /// ordinary game.
    ///
    /// A `bool` beside a `DevState` would have made "disabled, but with a row dialled
    /// to a million" a writable state, and `Option` is the same answer this crate
    /// already gives for the target cell and the running boost.
    #[cfg(debug_assertions)]
    pub dev: Option<DevState>,
    /// When the next simulation step falls due.
    ///
    /// **A deadline, not a countdown**, which is what makes the 20 tps rate survive a
    /// late wake-up: [`advance`](App::advance) runs steps *until* this passes `now`
    /// and adds [`SIM_PERIOD`] to it each time, so a frame that took 80 ms runs the
    /// step it owed and stays on the same grid. A remaining-time counter decremented
    /// by the elapsed time would instead drift by whatever each frame overshot, and a
    /// run's pace would depend on how busy the machine was.
    next_tick: Instant,
    /// When the mine key was last heard from, or [`None`] if it is up.
    ///
    /// The whole of "is the player mining", and deliberately one field rather than
    /// two paths. A terminal that reports releases writes [`None`] here the moment the
    /// key comes up; one that cannot lets [`HOLD_WINDOW`] answer instead. The exact
    /// path is an *early cut* of the inferred one, not a rival to it, so nothing
    /// downstream has to know which terminal it is running on.
    last_mine_key: Option<Instant>,
    /// What the keyboard said about that key since the last [`advance`](App::advance).
    ///
    /// Collapsed to the latest edge rather than queued: at a 10 ms sampling period, a
    /// press and a release inside one window is not a gesture a human can make, and a
    /// queue would be a mechanism for a case that cannot occur.
    mine_key_edge: Option<MineKeyEdge>,
}

impl App {
    /// A fresh session over `state`, opened on the Mine tab.
    ///
    /// **Takes the run rather than starting one**, and that is what keeps this
    /// testable. `GameState::new` needs a seed and a `now`, and the only honest
    /// source for both is the wall clock — which would make every assertion about a
    /// rendered frame depend on when the test ran. `main` reads the clock;
    /// tests pass a fixed seed and a fixed instant, and get the same grid every
    /// time.
    ///
    /// It is also the shape the save wants: phase 7 loads a `GameState` from disk
    /// and hands it here, where today's caller builds a new one.
    pub fn new(state: GameState) -> Self {
        // Seeded from the run, so opening the Mines tab highlights where the player
        // actually is rather than the top of the list — and the Upgrades ladder opens
        // on the rung they are standing on, which is the same question asked of the
        // other axis of progression.
        let cursors = Self::cursors_for(&state);
        // The simulation's clock starts one period out rather than due, so the first
        // step falls in one period from here rather than the instant the loop first
        // looks. The *frame* clock is [`Session`](crate::session::Session)'s and starts
        // due, which is what makes the opening pass draw.
        let now = Instant::now();
        // A session opens having announced nothing, so the log the first projection
        // reads is empty by construction rather than by an argument spelled `None`.
        let toasts = Toasts::new();
        // Nothing has fired yet, so the buffer is about no mine at all — which is the
        // state its `Default` already spells, rather than one this line has to choose.
        let flash = Flashes::new();
        let view = View::from_state(&state, cursors, None, &toasts, &flash, now);
        Self {
            leaving: None,
            screen: Screen::Mine,
            modal: None,
            toasts,
            flash,
            state,
            view,
            cursors,
            refused: None,
            config: Config::default(),
            #[cfg(debug_assertions)]
            dev: None,
            next_tick: now + SIM_PERIOD,
            last_mine_key: None,
            mine_key_edge: None,
        }
    }

    /// Turns the dev menu on for this session.
    ///
    /// **A builder step and not a parameter of [`new`](App::new)**, so that the
    /// hundred-odd tests that build an `App` say nothing about a feature they are not
    /// about — and so that the `#[cfg]` lives on one method instead of on every call
    /// site of the constructor. `main` is the only non-test caller.
    ///
    /// It takes a `bool` rather than being called conditionally because *whether* the
    /// environment asked for it is [`main`](crate::main)'s reading, and this is where
    /// that reading is spent.
    #[cfg(debug_assertions)]
    pub fn with_dev(mut self, enabled: bool) -> Self {
        self.dev = enabled.then(DevState::default);
        self
    }

    /// Opens this session on the preferences the save carried.
    ///
    /// **A builder step for [`with_dev`](App::with_dev)'s reason**: the hundred-odd
    /// tests that build an `App` are not about preferences, and a second parameter on
    /// [`new`](App::new) would make every one of them name a default.
    ///
    /// It exists because [`Config`] lives *inside* the save (`docs/SYSTEMS.md`
    /// §*Config in the save*): a loaded run brings its own, and only a genuinely new
    /// one falls back to [`Config::default`]. Applied after construction rather than
    /// during it because nothing `new` computes reads it — the read model is projected
    /// from the run, and the preferences are consulted while *drawing*.
    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// Rebuilds the read model from the run.
    ///
    /// Called before drawing, and only when a draw is actually about to happen: the
    /// guard is [`Session`](crate::session::Session)'s `dirty`, on the caller's side,
    /// so a pass that asks the terminal for nothing does not project a snapshot nobody
    /// reads. Which is what keeps this affordable now that a 20 tps tick changes the
    /// run whether the player touches anything or not.
    ///
    /// **`now` is the frame's instant, and it must be the same one the frame is drawn
    /// at.** The history's ages are computed here, the proc flash's beat is resolved
    /// here, and the toast is expired in [`render`](App::render); a second reading of the
    /// clock between them would let a log say `0s` about an announcement the very same
    /// frame had already stopped showing, or draw a blast one beat behind its own toast.
    ///
    /// `pub(crate)` and not private: the loop that decides when a frame is due moved
    /// out to [`Session`](crate::session::Session), and this is one of the three doors
    /// it needs. The visibility stops at the crate, so nothing outside can project a
    /// view at an instant of its own choosing.
    pub(crate) fn sync_view(&mut self, now: Instant) {
        self.view = View::from_state(
            &self.state,
            self.cursors,
            self.refused.as_ref(),
            &self.toasts,
            &self.flash,
            now,
        );
    }

    /// Applies one decoded intent.
    ///
    /// This is the reducer, and it is the reason [`Action`] exists: it takes no
    /// `KeyEvent` and touches no terminal, so every transition below is a plain
    /// unit test. The `match` is exhaustive, so a new [`Action`] variant cannot be
    /// added without deciding what it does here.
    ///
    /// **A modal is offered the gesture before the screen is, and that ordering is
    /// the rule.** A modal captures the keyboard — [`crate::keymap`] already gives it first
    /// refusal on every *key* — so it must also own what those keys decode to, or a
    /// `←` meant for the compression spinner would slide the richness dial on the
    /// screen behind the box. Phase 6's dip modal was the first to inherit the seam
    /// rather than re-derive it — it cost one arm — and phase 7's four remaining
    /// overlays will do the same, which is why the split lives here in one line and
    /// not as a condition repeated in each arm.
    pub fn update(&mut self, action: Action) {
        // Returns `true` when the stacked modal consumed the gesture, so the screen
        // below never sees it.
        if self.update_modal(&action) {
            return;
        }
        match action {
            Action::ToTitle => self.leaving = Some(Leaving::ToTitle),
            Action::Quit => self.leaving = Some(Leaving::Process),
            Action::NextScreen => self.screen = self.screen.next(),
            Action::PrevScreen => self.screen = self.screen.prev(),
            Action::SelectScreen(index) => {
                // An out-of-range index leaves the screen alone rather than
                // clamping: the ring is the authority on what exists.
                if let Some(screen) = Screen::from_index(index) {
                    self.screen = screen;
                }
            }
            // Recorded, not acted on. `update` has no clock — that is the property
            // every test of it relies on — so the instant is stamped by `advance`,
            // which is called immediately after this returns.
            Action::MinePressed => self.mine_key_edge = Some(MineKeyEdge::Down),
            Action::MineReleased => self.mine_key_edge = Some(MineKeyEdge::Up),
            // `?` stacks Help over the current screen; `Esc`/`?` clear it. The
            // keymap only emits `OpenHelp` when nothing is stacked, so this never
            // buries one modal under another.
            Action::OpenHelp => self.modal = Some(Modal::Help),
            Action::CloseModal => self.modal = None,
            // The list gestures are decoded without a screen in mind, so this is
            // where one is chosen. Mines, Inventory and Upgrades answer today;
            // phase 7 adds arms.
            Action::CursorUp => self.step_list_cursor(-1),
            Action::CursorDown => self.step_list_cursor(1),
            Action::AdjustLeft => {
                if self.screen == Screen::Mines {
                    self.step_richness_dial(-1);
                }
            }
            Action::AdjustRight => {
                if self.screen == Screen::Mines {
                    self.step_richness_dial(1);
                }
            }
            Action::Confirm => match self.screen {
                Screen::Mines => self.enter_selected_mine(),
                Screen::Upgrades => self.buy_at_cursor(Reach::ToCursor),
                Screen::Levels => self.claim_at_cursor(),
                _ => {}
            },
            Action::ClaimAll => {
                if self.screen == Screen::Levels {
                    self.claim_everything();
                }
            }
            // `Home`. Only the Levels roadmap has a "where you actually are" to jump
            // back to — the other lists are short enough to walk, and on Inventory or
            // Mines the player's position is already the row they opened on.
            Action::JumpToCurrent => {
                if self.screen == Screen::Levels {
                    self.cursors.level = self.state.player().get_level();
                }
            }
            // `M`, and only where something is for sale. It reaches the *same* buy as
            // `Enter` with a further target, which is what keeps "buy to here" and
            // "buy max" from being two implementations of one purchase.
            Action::BuyMax => {
                if self.screen == Screen::Upgrades {
                    self.buy_at_cursor(Reach::AsFarAsPossible);
                }
            }
            // The sub-tab ring. Guarded on the screen even though `keymap` only emits
            // these there, because the reducer is the place a gesture's meaning is
            // decided and a guard that lives in only one of the two is a guard that
            // moves when the binding does.
            Action::NextSubTab => {
                if self.screen == Screen::Upgrades {
                    self.cursors.upgrade_tab = self.cursors.upgrade_tab.next();
                }
            }
            Action::PrevSubTab => {
                if self.screen == Screen::Upgrades {
                    self.cursors.upgrade_tab = self.cursors.upgrade_tab.prev();
                }
            }
            // Nothing to adjust to its maximum outside the spinner, which
            // `update_modal` has already answered for.
            Action::AdjustMax => {}
            // Guarded on the screen even though `stats::map_key` is the only decoder
            // that emits it, for the reason the sub-tab arms are: the reducer is where
            // a gesture's meaning is settled, and a guard living only in the keymap
            // moves the day the binding does.
            Action::OpenPrestige => {
                if self.screen == Screen::Stats {
                    self.modal = Some(Modal::PrestigePreview);
                }
            }
            // Nothing outside the confirm's field takes text, and `update_modal` has
            // already answered for the one thing that does.
            Action::TypeChar(_) | Action::EraseChar => {}
            Action::Compress => self.open_conversion(Conversion::Compress),
            Action::Decompress => self.open_conversion(Conversion::Decompress),
            Action::GoCompress => self.walk_to_the_refused_pile(),
            // `keymap` only emits this when `dev` is `Some`, so there is no second
            // guard here: stacking the modal is safe either way — `render` draws
            // nothing without a `DevState` to draw.
            #[cfg(debug_assertions)]
            Action::OpenDevMenu => self.modal = Some(Modal::Dev),
        }
    }

    /// Walks to the Inventory, onto the pile the remembered refusal named.
    ///
    /// **The return leg of UI.md §8.4's loop**, whose two other legs were already
    /// built: the refusal is remembered in [`refused`](App::refused), and
    /// [`inventory_view`](crate::view) prints it on the row it names. What was missing
    /// was the walk itself, which the player made by hand — `3`, then `↑↓` down a
    /// fifteen-row table to find the material a toast had named three seconds ago and
    /// which was by then gone.
    ///
    /// **The refusal is not consumed.** It is what the Inventory screen prints the
    /// hint from, so clearing it here would empty the panel this walk exists to reach.
    /// It is cleared where it always was: by the next purchase outcome.
    ///
    /// The cursor only moves when there is somewhere to move it — a `c` pressed with
    /// nothing refused still travels, and lands where the player last left the table.
    /// The alternative was a key that does nothing at all, which is a worse answer to
    /// the same rare press.
    fn walk_to_the_refused_pile(&mut self) {
        if let Some(hint) = &self.refused {
            self.cursors.material = hint.needed.material;
        }
        self.screen = Screen::Inventory;
    }

    /// Offers `action` to the stacked modal, answering whether it took it.
    ///
    /// **Only the two modals with a *state* answer anything** — a spinner's count and
    /// a caret's side. Help swallows keys in [`crate::keymap`] and never reaches here with a
    /// gesture at all; `Esc` deliberately falls through to [`Action::CloseModal`] in
    /// the main `match`, so closing a modal stays one implementation for every modal
    /// there will ever be.
    ///
    /// Returning a `bool` rather than an `Option<Action>` to re-dispatch: a modal
    /// either consumed the gesture or did not, and translating one gesture into
    /// another would give the reducer a second dispatch path to reason about.
    fn update_modal(&mut self, action: &Action) -> bool {
        // **Cloned, where this used to copy.** [`Modal`] stopped being `Copy` when the
        // prestige confirm gained a `String`, and every arm below both reads the modal
        // and reassigns it — so a borrow of `self.modal` would still be live where the
        // arm writes to it, which the borrow checker refuses. The clone is one small
        // allocation per keystroke, on the input path and never on the render one.
        match self.modal.clone() {
            Some(Modal::Compress {
                material,
                direction,
                units,
            }) => {
                match action {
                    // `saturating_sub` on the way down and a clamp on the way up: the
                    // floor is 1, since a conversion of nothing is not something the
                    // dialog should be able to offer, and `Esc` is how a player who
                    // changed their mind leaves.
                    Action::AdjustLeft => {
                        self.set_spinner(material, direction, units.saturating_sub(1));
                    }
                    Action::AdjustRight => {
                        self.set_spinner(material, direction, units.saturating_add(1));
                    }
                    // `a` — *all*. Asking for more than the pile holds and letting the
                    // clamp answer, rather than reading the ceiling twice.
                    Action::AdjustMax => self.set_spinner(material, direction, u32::MAX),
                    Action::Confirm => self.apply_conversion(material, direction, units),
                    _ => return false,
                }
                true
            }
            // Two options, so the gestures **clamp** rather than wrap: a caret that
            // rolled off `Not yet` onto `Buy it` would put the dangerous option one
            // repeat of a held key away from the safe one.
            Some(Modal::Dip { to, buy }) => {
                match action {
                    Action::AdjustLeft => self.modal = Some(Modal::Dip { to, buy: true }),
                    Action::AdjustRight => self.modal = Some(Modal::Dip { to, buy: false }),
                    Action::Confirm => self.confirm_dip(to, buy),
                    _ => return false,
                }
                true
            }
            // The preview answers one gesture. `Esc` is not here: it falls through to
            // the single `CloseModal` arm every modal shares, which is what keeps
            // closing a box one implementation for all six of them.
            Some(Modal::PrestigePreview) => {
                match action {
                    Action::Confirm => self.open_prestige_confirm(),
                    // The §8.4 walk, and the modal closes on the way out: the player is
                    // leaving for another screen, and a box left stacked over it would
                    // capture the very keys they went there to press.
                    Action::GoCompress => {
                        self.modal = None;
                        self.walk_to_the_refused_pile();
                    }
                    _ => return false,
                }
                true
            }
            // The field. `TypeChar` and `EraseChar` exist for this arm and no other.
            Some(Modal::PrestigeConfirm { typed }) => {
                match action {
                    Action::TypeChar(character) => self.type_into_confirm(typed, *character),
                    Action::EraseChar => {
                        let mut typed = typed;
                        typed.pop();
                        self.modal = Some(Modal::PrestigeConfirm { typed });
                    }
                    Action::Confirm => self.confirm_prestige(&typed),
                    _ => return false,
                }
                true
            }
            // The dev menu reuses the four list gestures and adds none: `↑↓` walks the
            // rows, `←→` turns the value under the cursor, `Enter` applies it. The
            // reason it can is the reason the compression dialog could — a gesture
            // names a movement, not a screen.
            #[cfg(debug_assertions)]
            Some(Modal::Dev) => {
                // A menu with no state is a menu that is not enabled, and `keymap`
                // cannot have produced the key that opened it. Closing is the honest
                // answer rather than a silent swallow.
                let Some(mut dev) = self.dev.take() else {
                    self.modal = None;
                    return true;
                };
                let handled = match action {
                    Action::CursorUp => {
                        dev.step_row(-1);
                        true
                    }
                    Action::CursorDown => {
                        dev.step_row(1);
                        true
                    }
                    // One arm for both directions, because the announcement below is
                    // owed by *whichever* of them flipped the toggle — and the toggle
                    // has no direction (`DevState::adjust` flips on either).
                    Action::AdjustLeft | Action::AdjustRight => {
                        let delta = if matches!(action, Action::AdjustLeft) {
                            -1
                        } else {
                            1
                        };
                        dev.adjust(delta);
                        // **The free-upgrades mode is announced when it changes**, the
                        // third of the three channels that carry it: the tab row's
                        // colour is the persistent one, the row's own `◄ on ►` is the
                        // authoritative one, and this is the one that catches the press.
                        // Three, because the marker has three columns and no room to say
                        // it in words — see `dev::MARKER`.
                        if dev.row == DevRow::FreeUpgrades {
                            let state = if dev.free_upgrades { "on" } else { "off" };
                            self.toasts.push(
                                format!("Free upgrades {state}"),
                                Tone::Neutral,
                                TOAST_TTL,
                            );
                        }
                        true
                    }
                    Action::Confirm => {
                        self.apply_dev_row(&dev);
                        true
                    }
                    _ => false,
                };
                // Put it back whatever happened — `take` is how the row can be applied
                // against `&mut self` at all, and a gesture this modal does not use
                // (`Esc`, falling through to `CloseModal`) must not cost the dialled
                // values.
                self.dev = Some(dev);
                handled
            }
            _ => false,
        }
    }

    /// Applies the dev row under the cursor, and announces what it did.
    ///
    /// **Takes the state by reference rather than reading `self.dev`**, because the
    /// caller has just moved it out: every door below needs `&mut self.state`, and a
    /// menu still borrowed from `self` would keep the whole `App` borrowed while the run
    /// was mutated. It is the same manoeuvre `set_spinner` makes by taking the three
    /// values it was destructured from.
    ///
    /// Every arm ends in a toast, including the ones that cannot fail. A dev tool whose
    /// keypress produced no visible change would send its user to check whether the key
    /// was even bound — and two of these rows (a level set to where it already is, a
    /// grant of a pile that is off screen) genuinely change nothing on the frame behind
    /// the box.
    ///
    /// **The two rows that reach a rule keep the rule's own words.** A level-up here
    /// goes through [`announce::of`], the same wording a mined one
    /// gets, and the offline skip prints [`OfflineReport`]'s figures rather than
    /// re-deriving them. `docs/DEV-MENU.md` records that as the module's one rule about
    /// itself: a dev path must not be able to say something the game would not.
    ///
    /// [`OfflineReport`]: skylode_core::game::OfflineReport
    #[cfg(debug_assertions)]
    fn apply_dev_row(&mut self, dev: &DevState) {
        let amount = dev.amount();
        let message = match dev.row {
            DevRow::FreeUpgrades => {
                // The toggle is turned by `←→`, so `Enter` on this row has nothing left
                // to do; saying what the row now reads is better than nothing happening.
                let state = if dev.free_upgrades { "on" } else { "off" };
                format!("Free upgrades {state}")
            }
            // The value row: the two rows below it spend this, and `Enter` here is the
            // same non-act as on the toggle.
            DevRow::Amount => format!("Amount {}", grouped(amount)),
            DevRow::Material => {
                let item = Item::Raw(dev.material);
                self.state.dev_grant(item, amount);
                format!("+{} {item}", grouped(amount))
            }
            DevRow::Everything => {
                self.state.dev_grant_all(amount);
                format!("+{} of all {} piles", grouped(amount), Material::ALL.len())
            }
            DevRow::Experience => {
                let events = self.state.dev_add_experience(amount);
                // The level-ups are announced in the game's own words, one toast each,
                // and this row's own sentence is the experience it granted.
                for event in &events {
                    let (text, tone) = announce::of(event);
                    self.toasts.push(text, tone, TOAST_TTL);
                }
                format!("+{} xp", grouped(amount))
            }
            DevRow::Level => {
                self.state.dev_set_level(dev.level);
                format!("Level {}", self.state.player().get_level())
            }
            DevRow::Prestige => {
                self.state.dev_set_prestige(dev.prestige);
                format!("Prestige rank {}", dev.prestige)
            }
            DevRow::Charges => {
                self.state.dev_grant_boost_charges(dev.charges);
                format!("+{} boost charges", dev.charges)
            }
            DevRow::SkipTime => self.skip_ahead(dev.skip(), dev.skip_label()),
        };
        // No redraw is asked for here, and none is needed: this is reached from
        // `update`, and the loop already redraws on any key that resolved to an
        // action. The flag it used to raise lives in `Session` now, and reaching up
        // into the loop to raise it again would be saying twice what the key already
        // said once.
        self.toasts.push(message, Tone::Success, TOAST_TTL);
    }

    /// Rewinds the offline mark by `by` and resumes, returning what was credited.
    ///
    /// **Two calls, and both are the shipped ones.** `dev_rewind` moves the mark;
    /// [`GameState::resume`] does the arithmetic, applies the cap and builds the report —
    /// the same path a relaunch takes. Nothing here multiplies anything, which is the
    /// whole reason the core needed no new rule for this row.
    ///
    /// A `None` from `resume` is reported rather than swallowed: it means the span
    /// credited nothing, and on this row that is a fact about the skip (a zero-length
    /// ladder entry could not exist, so it means the mark was already at the epoch).
    ///
    /// [`GameState::resume`]: skylode_core::game::GameState::resume
    #[cfg(debug_assertions)]
    fn skip_ahead(&mut self, by: Duration, label: &str) -> String {
        let now = self.state.last_seen();
        self.state.dev_rewind(by);
        match self.state.resume(now) {
            Some(report) => format!(
                "Skipped {label} — {} blocks mined",
                grouped_u64(report.blocks)
            ),
            None => "Skipped nothing — the mark is already at the epoch".to_owned(),
        }
    }

    /// Takes the focused option of the dip modal, and closes it either way.
    ///
    /// **Declining is a close and nothing else**, which is why there is no toast on
    /// that side: the player asked a question, read the answer, and said no — a line
    /// announcing that they did not buy anything would be the interface talking about
    /// itself. Buying goes through the same [`buy_pickaxe_chain`](App::buy_pickaxe_chain)
    /// path as an undipped purchase, so there is one implementation of *what a chain
    /// costs and announces* and the modal only decides whether it runs.
    ///
    /// **Takes the pair rather than re-reading `self.modal`**, the same device
    /// [`set_spinner`](App::set_spinner) uses: the caller has just destructured it, so
    /// reading it again would need an `else { return }` for a case the caller has
    /// proved impossible — a branch no test could justify.
    fn confirm_dip(&mut self, to: usize, buy: bool) {
        self.modal = None;
        if buy {
            self.climb_to(to);
        }
    }

    /// Moves from the prestige preview to the typed confirm, or says why not.
    ///
    /// **The preview stays up on a refusal.** It is the screen that explains the
    /// refusal — the `✗`, the shut gates, the shortfall in its closing line — so
    /// closing it would take the answer away with the question. The toast is raised on
    /// top of that, which is Enoal's call: the box is 68×18 and the toast sits three
    /// rows off the bottom edge, so the sentence is legible under it.
    ///
    /// **Both halves are re-read here rather than trusted from the projection.** The
    /// `View` is rebuilt before each *draw*, and a tick between the last draw and this
    /// keypress can have credited the ore that opens the trade; the till and the lock
    /// are cheap and cannot be stale.
    fn open_prestige_confirm(&mut self) {
        let player = self.state.player();
        let lock = player.prestige_lock();
        if !lock.is_open() {
            // The core's own sentence, not a second one written here — the same
            // `Display` `GameState::prestige` would have refused with.
            self.announce_core_refusal(Err(CoreError::PrestigeLocked { lock }));
            return;
        }
        let rank = player.get_prestige();
        let cost = prestige::cost(rank);
        let verdict = economy::affordability(player.get_inventory(), &cost);
        if verdict == Affordability::Affordable {
            self.modal = Some(Modal::PrestigeConfirm {
                typed: String::new(),
            });
            // A trade about to happen is not a refusal to walk away from.
            self.refused = None;
        } else {
            self.announce_refusal(&verdict);
            // **The §8.4 loop, joined by its fourth door.** A prestige is refused for the
            // denomination exactly as the four purchase tracks are, so it must leave the
            // same memory behind: without it, `c` walks to an Inventory screen that says
            // nothing about what the player came for, and the loop is a walk with no
            // errand at the end. `remember_refusal` clears it on every other outcome, so
            // this is also what stops a stale note surviving the trade.
            let purchase = format!("Prestige {}", prestige_rank(rank.saturating_add(1)));
            self.remember_refusal(&purchase, &cost);
        }
    }

    /// Appends `character` to the confirm's field, up to the length of the word.
    ///
    /// **Capped at [`CONFIRM_WORD`]'s length and not at the field's drawn width.** The
    /// field is twelve columns because the frame draws it so, but nothing longer than
    /// the word can ever be right, and a field that kept accepting letters would let a
    /// player type past their own mistake instead of meeting it. `Backspace` is the way
    /// back, which is why the cap can be this tight.
    ///
    /// The character is taken exactly as typed: no upper-casing, because §6.9's
    /// argument is that the word must be *typed*, and quietly correcting a lower-case
    /// `p` into a `P` is the interface doing half of it for them.
    fn type_into_confirm(&mut self, mut typed: String, character: char) {
        if typed.chars().count() < CONFIRM_WORD.chars().count() {
            typed.push(character);
        }
        self.modal = Some(Modal::PrestigeConfirm { typed });
    }

    /// Trades the run in, if the word is right.
    ///
    /// **A wrong word is silent.** The field is on screen with what the player typed in
    /// it, beside the word they were asked for, so there is nothing a toast could add —
    /// and a toast raised here would be drawn under the box that already answers it.
    /// That is the richness dial's rule: a refusal the player can see is not announced.
    ///
    /// The `Result` is routed rather than assumed. Nothing a tick does can shut a gate
    /// or spend the player's Amethyst, so the refusal is unreachable today; routing it
    /// costs one arm and is what keeps the till — not this method — the authority on
    /// whether the trade happens.
    fn confirm_prestige(&mut self, typed: &str) {
        if typed != CONFIRM_WORD {
            return;
        }
        self.modal = None;
        let rank = self.state.player().get_prestige().saturating_add(1);
        match self.state.prestige() {
            Ok(()) => {
                // The run is not the one the front-end was pointing at any more: the
                // pickaxe is Wooden, the level is 1, the mines the player left behind
                // are gone. Rebuilt through the *same* call `new` makes, so a cursor
                // cannot open somewhere after a prestige that it could not open on a
                // fresh run.
                self.cursors = Self::cursors_for(&self.state);
                // A remembered compress-first refusal names a price in an inventory
                // that no longer exists.
                self.refused = None;
                // §8.1's own edge: the confirm leads to the Mine screen, because what
                // the player has just bought is a run to walk again.
                self.screen = Screen::Mine;
                self.toasts.push(
                    format!(
                        "Prestige {} — {} on everything",
                        prestige_rank(rank),
                        multiplier(prestige::multiplier_permille(rank))
                    ),
                    Tone::Success,
                    TOAST_TTL,
                );
            }
            outcome => self.announce_core_refusal(outcome),
        }
    }

    /// Where each cursor opens on `state`.
    ///
    /// **Extracted so there is one answer**, not because two callers happened to want
    /// the same three lines: [`new`](App::new) sets them when a session starts and
    /// [`confirm_prestige`](App::confirm_prestige) resets them when the run underneath
    /// them is replaced, and those are the same question asked twice. A second copy
    /// would let a post-prestige session open on a rung the player no longer stands on.
    fn cursors_for(state: &GameState) -> Cursors {
        Cursors::new(
            state.current_mine().kind(),
            upgrade::position(&upgrade::ladder(), state.player().get_pickaxe()),
            state.player().get_level(),
        )
    }

    /// Moves the spinner to `requested`, clamped into what the pile can actually
    /// convert.
    ///
    /// **The ceiling is re-read from the inventory on every step** rather than stored
    /// beside the count when the dialog opened. It costs a division, and it means the
    /// bound cannot go stale — the tick credits loot while a modal is up, so a ceiling
    /// captured at opening time is wrong by the second keypress.
    ///
    /// `max(1)` on the ceiling because [`u32::clamp`] panics when its floor exceeds
    /// its ceiling. A dialog is only ever *opened* on a pile of at least one unit, so
    /// the zero case belongs to a pile emptied underneath it — a confirm that spent
    /// the lot — and the honest answer there is to leave the spinner reading 1 rather
    /// than to take the terminal down over it.
    ///
    /// **Takes the pair rather than re-reading it off `self.modal`**, because
    /// [`update_modal`](App::update_modal) has already destructured it: reading it a
    /// second time would need a second `else { return }` for a case the caller has
    /// just proved impossible, and a branch nothing can reach is a branch no test can
    /// justify.
    fn set_spinner(&mut self, material: Material, direction: Conversion, requested: u32) {
        let ceiling =
            compression::max_units(self.state.player().get_inventory(), material, direction).max(1);
        self.modal = Some(Modal::Compress {
            material,
            direction,
            units: requested.clamp(1, ceiling),
        });
    }

    /// Opens the dialog on the material under the Inventory cursor, or says why
    /// there is nothing to open it for.
    ///
    /// **A pile with nothing to convert gets a toast and no dialog.** A modal the
    /// player can only cancel is a keypress spent on nothing, and the refusal is one
    /// they cannot otherwise see — the panel prints `Compressible now: 0`, but a
    /// player who pressed `c` was not reading it.
    ///
    /// The ceiling is asked of [`compression::max_units`], the same function the
    /// dialog draws its `all (13)` from, so "is there anything to convert" and "how
    /// much" are one answer read twice. It is display arithmetic and not a rule: the
    /// Compress panel already performs the same division.
    fn open_conversion(&mut self, direction: Conversion) {
        if self.screen != Screen::Inventory {
            return;
        }
        let material = self.cursors.material;
        let inventory = self.state.player().get_inventory();
        if compression::max_units(inventory, material, direction) > 0 {
            // Opens at one, the smallest real conversion, so a player who hits
            // `Enter` straight away never converts more than they meant to. `a` is
            // one keypress away for the other end.
            self.modal = Some(Modal::Compress {
                material,
                direction,
                units: 1,
            });
            return;
        }

        let name = material.name();
        let refusal = match direction {
            Conversion::Compress => format!(
                "Nothing to compress — {RAW_PER_COMPRESSED} raw {name} needed, {} held",
                grouped(inventory.count(Item::Raw(material)))
            ),
            Conversion::Decompress => format!("Nothing to decompress — no Compressed {name} held"),
        };
        self.toasts.push(refusal, Tone::Refusal, TOAST_TTL);
    }

    /// Performs the conversion the dialog is set to, announces it, and closes.
    ///
    /// **The announcement names the [`Item`] that was gained, not a sentence per
    /// direction**, which is what makes one `format!` serve both: `Item`'s own
    /// [`Display`](std::fmt::Display) already writes `Compressed Iron` for one
    /// denomination and `Iron` for the other, so the wording of a denomination lives
    /// in the core beside the type and cannot drift between the two toasts here. The
    /// `+N` shape is the house style the event toasts already use.
    ///
    /// A refusal is toasted verbatim: [`CoreError`]
    /// says what it refused and why, and the dialog closes either way — the state it
    /// was set against has just been proved wrong, so leaving it up would invite the
    /// player to press `Enter` again on the same impossible number.
    fn apply_conversion(&mut self, material: Material, direction: Conversion, units: u32) {
        let (outcome, gained) = match direction {
            Conversion::Compress => (
                self.state.compress(material, units),
                (Item::Compressed(material), units),
            ),
            Conversion::Decompress => (
                self.state.decompress(material, units),
                (
                    Item::Raw(material),
                    units.saturating_mul(RAW_PER_COMPRESSED),
                ),
            ),
        };

        // The tone travels with the sentence rather than being derived from it after
        // the fact: the `Err` arm prints whatever the core said, and matching that
        // string back to a colour would be a parser standing where a `Result` already
        // is.
        let (message, tone) = match outcome {
            Ok(()) => {
                let (item, amount) = gained;
                (format!("+{} {item}", grouped(amount)), Tone::Success)
            }
            Err(refusal) => (refusal.to_string(), Tone::Refusal),
        };
        self.toasts.push(message, tone, TOAST_TTL);
        self.modal = None;
    }

    /// Buys whatever the Upgrades cursor is sitting on, and announces the outcome.
    ///
    /// **One entry point for `Enter` and `M`**, because they differ only in *how far*:
    /// [`Reach`] is that difference and nothing else. Two functions would be two
    /// places for the refusal wording, the toast and the dip check to drift.
    fn buy_at_cursor(&mut self, reach: Reach) {
        // Free upgrades are not a *discount* applied on the way to the till: they are
        // a different set of doors, the ones `skylode_core::game::dev` opens, which
        // never consult an inventory at all. Topping the wallet up instead would have
        // been the other design, and it fails on its own terms — a purse holding
        // billions makes every price on the screen unreadable, which is the screen this
        // is meant to let you look at.
        #[cfg(debug_assertions)]
        if self.dev.as_ref().is_some_and(|dev| dev.free_upgrades) {
            self.buy_free_at_cursor(reach);
            return;
        }
        match self.cursors.upgrade_tab {
            UpgradeTab::Pickaxe => self.buy_pickaxe_chain(reach),
            UpgradeTab::Enchants => self.buy_enchant_levels(reach),
            UpgradeTab::Mines => self.buy_mine_track(reach),
        }
    }

    /// Buys the same thing [`buy_at_cursor`](App::buy_at_cursor) would, through the
    /// free doors.
    ///
    /// **The one place free mode differs from paid mode, and the differences it does
    /// *not* make are the point.** The cursor still decides what is bought, the sub-tab
    /// still decides which track, `M` still means "as far as this goes", and the caps
    /// still refuse — [`GameState::dev_upgrade_pickaxe`] stops at Netherite Efficiency
    /// 15 exactly as the paid door does, because neither of them was ever the thing
    /// enforcing it.
    ///
    /// **The dip modal is deliberately skipped.** Its question is *"this purchase costs
    /// you power — spend the ore anyway?"*, and with nothing spent the question has no
    /// second half. A confirm that could only be answered yes is a keypress.
    ///
    /// Announced by the outcome and not by a count: a climb that moved is named by
    /// where it arrived, which is the same sentence the paid path prints, and one that
    /// did not moved because a cap said so — so the cap's own refusal is the news.
    ///
    /// [`GameState::dev_upgrade_pickaxe`]: skylode_core::game::GameState::dev_upgrade_pickaxe
    #[cfg(debug_assertions)]
    fn buy_free_at_cursor(&mut self, reach: Reach) {
        self.refused = None;
        let (moved, refusal) = match self.cursors.upgrade_tab {
            UpgradeTab::Pickaxe => {
                // **Rung by rung, and counted rather than aimed.** The paid path climbs
                // *to* a ladder index because each rung has to be priced from where the
                // last one left off; nothing here has a price, so the only thing the
                // ladder is still needed for is the distance to the cursor. `M` does not
                // consult it at all — on a free track "as far as possible" is *until it
                // refuses*, and the only thing that can refuse a free climb is the cap
                // at the top.
                let wanted = match reach {
                    Reach::AsFarAsPossible => u32::MAX,
                    Reach::ToCursor => {
                        let ladder = upgrade::ladder();
                        let from = upgrade::position(&ladder, self.state.player().get_pickaxe());
                        u32::try_from(self.cursors.pickaxe_rung.saturating_sub(from))
                            .unwrap_or(u32::MAX)
                    }
                };
                self.repeat_free(wanted, |state| state.dev_upgrade_pickaxe())
            }
            UpgradeTab::Enchants => {
                let kind = self.cursors.enchant;
                self.repeat_free(Self::steps(reach), |state| state.dev_upgrade_enchant(kind))
            }
            UpgradeTab::Mines => {
                let (kind, track) = self.cursors.mine_track;
                self.repeat_free(Self::steps(reach), |state| match track {
                    MineTrack::Size => state.dev_upgrade_mine_size(kind),
                    MineTrack::Richness => state.dev_upgrade_mine_richness(kind),
                })
            }
        };

        // **Three outcomes and not two.** A refusal after a partial climb is still a
        // climb, and reporting the cap that stopped it would bury the four rungs that
        // did land; a target already reached refuses nothing and buys nothing, and
        // calling that a refusal would put a red toast on a keypress that was simply
        // early.
        let (message, tone) = match (moved, refusal) {
            (0, Some(error)) => (error.to_string(), Tone::Refusal),
            (0, None) => (
                "Nothing left to buy on this track".to_owned(),
                Tone::Neutral,
            ),
            _ => (self.free_purchase_label(), Tone::Success),
        };
        self.toasts.push(message, tone, TOAST_TTL);
    }

    /// How many free steps a [`Reach`] asks for on a track whose rungs are independent.
    ///
    /// The enchant and mine tracks price every level from the one below it, so "to the
    /// cursor" is always exactly one step and "as far as possible" is until the cap
    /// refuses. Only the pickaxe ladder has a *distance*, and it computes its own.
    #[cfg(debug_assertions)]
    fn steps(reach: Reach) -> u32 {
        match reach {
            Reach::ToCursor => 1,
            Reach::AsFarAsPossible => u32::MAX,
        }
    }

    /// Names where the active Upgrades track now stands.
    ///
    /// Read *after* the purchase and off the run, never composed from what was asked
    /// for: it is [`climb_to`](App::climb_to)'s rule — "Bought Netherite Pickaxe" is
    /// what the player was looking at, and a sentence built from the target would name
    /// a rung a partial climb never reached.
    #[cfg(debug_assertions)]
    fn free_purchase_label(&self) -> String {
        match self.cursors.upgrade_tab {
            UpgradeTab::Pickaxe => {
                let pickaxe = self.state.player().get_pickaxe();
                let label = rung_label(
                    pickaxe.get_tier(),
                    pickaxe.enchants().get_level(EnchantType::Efficiency),
                );
                format!("Bought {label}")
            }
            UpgradeTab::Enchants => {
                let kind = self.cursors.enchant;
                let level = self.state.player().get_pickaxe().enchants().get_level(kind);
                format!("Bought {} {}", kind.name(), roman(level))
            }
            UpgradeTab::Mines => {
                let (kind, track) = self.cursors.mine_track;
                let what = match track {
                    MineTrack::Size => "size",
                    MineTrack::Richness => "richness",
                };
                let level = self.state.mine(kind).map_or(0, |mine| match track {
                    MineTrack::Size => mine.get_size_level(),
                    MineTrack::Richness => mine.get_richness_level(),
                });
                format!("{} {what} → level {level}", kind.name())
            }
        }
    }

    /// Runs a free purchase `wanted` times, or until it refuses, and reports both
    /// halves.
    ///
    /// The free counterpart of [`economy::buy_repeatedly`], which cannot be reused
    /// here: that one discards the refusal, and a free purchase has nothing *but* the
    /// refusal to report — there is no shortfall to word the news from.
    ///
    /// Takes a count rather than a [`Reach`], because one of the three tracks turns a
    /// `Reach` into a *distance* and the other two into `1`. Passing the enum in would
    /// mean this function knew which track it was serving.
    #[cfg(debug_assertions)]
    fn repeat_free(
        &mut self,
        wanted: u32,
        mut buy: impl FnMut(&mut GameState) -> Result<(), skylode_core::error::CoreError>,
    ) -> (u32, Option<skylode_core::error::CoreError>) {
        let mut moved = 0;
        while moved < wanted {
            match buy(&mut self.state) {
                Ok(()) => moved += 1,
                Err(error) => return (moved, Some(error)),
            }
        }
        (moved, None)
    }

    /// Climbs the pickaxe roadmap to the cursor, or as far as the ore reaches —
    /// stopping to ask first when the climb would cost power.
    ///
    /// **The question is asked here and not in [`climb_to`](App::climb_to)**, so the
    /// modal's own confirm can reach the purchase without meeting its own guard again.
    /// Asked of [`upgrade::preview`], which is the core's single definition of a dip:
    /// a front-end that re-derived "did we lose power" would be free to disagree with
    /// the pane that has just drawn the box.
    fn buy_pickaxe_chain(&mut self, reach: Reach) {
        let inventory = self.state.player().get_inventory();
        let pickaxe = self.state.player().get_pickaxe();
        let from = upgrade::position(&upgrade::ladder(), pickaxe);
        let to = match reach {
            Reach::ToCursor => self.cursors.pickaxe_rung,
            // **At least the next rung, even when nothing is affordable.** `M` on a
            // penniless run would otherwise target the rung the player is already on,
            // and the chain to *there* is affordable by definition — so the screen
            // would answer "nothing to buy" to a player whose actual problem is that
            // they are short of ore. Aiming one rung further makes the refusal the
            // real one.
            Reach::AsFarAsPossible => {
                upgrade::max_affordable(inventory, pickaxe).max(from.saturating_add(1))
            }
        };

        if upgrade::preview(pickaxe, to).is_dip() {
            self.modal = Some(Modal::Dip { to, buy: false });
            return;
        }
        self.climb_to(to);
    }

    /// Buys the chain to `to` and announces what happened.
    ///
    /// **The refusal is read from [`upgrade::chain_affordability`] and not from the
    /// purchase**, which is the §8.4 rule: the two branches are different news and the
    /// core already computed which one applies, with the shortfall attached. Asking
    /// after a `0`-rung climb is asking the same question the row's mark answered, so
    /// the toast and the `✓ ~ ✗` cannot contradict each other.
    fn climb_to(&mut self, to: usize) {
        if self.state.buy_pickaxe_chain(to) == 0 {
            let refusal = upgrade::chain_affordability(
                self.state.player().get_inventory(),
                self.state.player().get_pickaxe(),
                to,
            );
            self.announce_refusal(&refusal);
            // **The rung that stopped the chain, not the one aimed at.** A chain is
            // simulated rung by rung, so the price the player is actually short of is
            // the first unaffordable one's — and the ladder answers where that is
            // without a second walk: it is one past where the affordable prefix ends.
            self.remember_chain_refusal();
            return;
        }

        // **The rung the pickaxe is *on*, read off the pickaxe** — not the count, and
        // not a lookup back into the ladder. "Bought Netherite Pickaxe" is what the
        // player was looking at; "bought 6 rungs" is what the loop did. Naming it from
        // the tool means there is no index that could miss, and therefore no fallback
        // sentence for a case that cannot happen.
        let pickaxe = self.state.player().get_pickaxe();
        let label = rung_label(
            pickaxe.get_tier(),
            pickaxe.enchants().get_level(EnchantType::Efficiency),
        );
        self.refused = None;
        self.toasts
            .push(format!("Bought {label}"), Tone::Success, TOAST_TTL);
    }

    /// Records the compress-first hint for the rung a chain stopped at, if any.
    ///
    /// Split out of [`climb_to`](App::climb_to) because the ladder has to be walked to
    /// find the rung and then indexed to price it, and doing that inline would put four
    /// lines of lookup between the refusal and the toast that announces it.
    fn remember_chain_refusal(&mut self) {
        let ladder = upgrade::ladder();
        let blocked = upgrade::max_affordable(
            self.state.player().get_inventory(),
            self.state.player().get_pickaxe(),
        )
        .saturating_add(1);
        let Some(cost) = ladder.get(blocked).and_then(|rung| rung.cost.clone()) else {
            self.refused = None;
            return;
        };
        let label = ladder
            .get(blocked)
            .map_or_else(String::new, |rung| rung_label(rung.tier, rung.efficiency));
        self.remember_refusal(&label, &cost);
    }

    /// Buys one level of the enchant under the cursor, or every level it can reach.
    ///
    /// `M` here means *buy to cap* rather than *buy to the end of a chain*, and the
    /// two are the same act on a track where every level is independently priced:
    /// [`economy::buy_repeatedly`] stops at the first refusal, which is the cap or the
    /// purse, whichever comes first.
    fn buy_enchant_levels(&mut self, reach: Reach) {
        let kind = self.cursors.enchant;
        let wanted = match reach {
            Reach::ToCursor => 1,
            Reach::AsFarAsPossible => u32::MAX,
        };
        let bought = economy::buy_repeatedly(wanted, || self.state.buy_enchant(kind));
        if bought == 0 {
            // Asked again for the *reason*, which is free: a refusal changes nothing,
            // so the second call re-derives the same `Err` against the same state.
            //
            // Bound to a local first because `self.announce_core_refusal(self.state
            // .buy_enchant(kind))` does not compile: the receiver borrows all of
            // `self` mutably before the argument is evaluated, and the argument wants
            // `self.state` mutably too. Two-phase borrows do not reach through a
            // `&mut self` method call.
            let refusal = self.state.buy_enchant(kind);
            let player = self.state.player();
            let level = player.get_pickaxe().enchants().get_level(kind);
            let cost = economy::enchant_cost(kind, level);
            self.announce_purchase_refusal(refusal, cost.as_ref());
            if let Some(cost) = cost {
                self.remember_refusal(&format!("{} {}", kind.name(), roman(level + 1)), &cost);
            }
            return;
        }

        let level = self.state.player().get_pickaxe().enchants().get_level(kind);
        self.refused = None;
        self.toasts.push(
            format!("Bought {} {}", kind.name(), roman(level)),
            Tone::Success,
            TOAST_TTL,
        );
    }

    /// Buys the next level of the mine track under the cursor, or every level it can
    /// reach.
    fn buy_mine_track(&mut self, reach: Reach) {
        let (kind, track) = self.cursors.mine_track;
        let wanted = match reach {
            Reach::ToCursor => 1,
            Reach::AsFarAsPossible => u32::MAX,
        };
        let what = match track {
            MineTrack::Size => "size",
            MineTrack::Richness => "richness",
        };
        let bought = economy::buy_repeatedly(wanted, || match track {
            MineTrack::Size => self.state.buy_mine_size(kind),
            MineTrack::Richness => self.state.buy_mine_richness(kind),
        });
        if bought == 0 {
            let refusal = match track {
                MineTrack::Size => self.state.buy_mine_size(kind),
                MineTrack::Richness => self.state.buy_mine_richness(kind),
            };
            // A mine this run never entered has no level to price from, so there is
            // nothing to compress *for* — and no price to word the refusal in either,
            // which is what leaves `MineNotEntered` speaking the core's own sentence.
            let priced = self.state.mine(kind).map(|mine| match track {
                MineTrack::Size => mine.get_size_level(),
                MineTrack::Richness => mine.get_richness_level(),
            });
            let cost = priced.map(|level| match track {
                MineTrack::Size => economy::mine_size_cost(kind, level),
                MineTrack::Richness => economy::mine_richness_cost(kind, level),
            });
            self.announce_purchase_refusal(refusal, cost.as_ref());
            if let (Some(level), Some(cost)) = (priced, cost) {
                // The rung being bought, named as the Upgrades pane names it: the
                // step is `level + 1`, and the display counts from 1 on top of that.
                let label = format!("{} {what} {}", kind.name(), shown_rung(level + 1));
                self.remember_refusal(&label, &cost);
            }
            return;
        }

        let level = self.state.mine(kind).map_or(0, |mine| match track {
            MineTrack::Size => mine.get_size_level(),
            MineTrack::Richness => mine.get_richness_level(),
        });
        self.refused = None;
        self.toasts.push(
            format!("{} {what} → level {level}", kind.name()),
            Tone::Success,
            TOAST_TTL,
        );
    }

    /// Toasts an [`Affordability`] verdict in the words its branch calls for.
    ///
    /// **The two refusals are different news** (`docs/UI.md` §8.4), and the wording is
    /// what routes the player: `Insufficient` sends them back to a mine,
    /// `CompressFirst` to the Inventory screen. The shortfall named is the *first*
    /// one — a price short in three materials is still one trip, and three toasts
    /// stacked on top of each other are three the player reads none of.
    fn announce_refusal(&mut self, verdict: &Affordability) {
        // **The tone is the branch, not a reading of the sentence.** The verdict is
        // already the three-way answer the `✓ ~ ✗` column draws, so a toast takes the
        // colour its own mark would have taken — one table, and a refusal that could
        // not be miscoloured by a reworded sentence.
        let (message, tone) = match verdict {
            // Reachable when the chain is affordable and bought nothing anyway, which
            // means there was nothing to buy: the cursor is at or behind the rung the
            // player stands on. Neutral rather than green — nothing was bought.
            Affordability::Affordable => ("Nothing to buy here".to_owned(), Tone::Neutral),
            // **The one refusal that ends in a keystroke**, and the toast is where that
            // keystroke is advertised. `c` walks to the pile named right here, so the
            // sentence that identifies the problem is also the one that hands over the
            // fix — and it is the only place it can be: a footer would have to carry a
            // fourth binding on a row that is already 75 columns of an 80-column frame,
            // for a key that is dead until something is refused.
            Affordability::CompressFirst(shortfalls) => (
                match shortfalls.first() {
                    Some(Shortfall { item, needed, held }) => format!(
                        "Compress first — need {} {item}, you have {} · c to go",
                        grouped(*needed),
                        grouped(*held)
                    ),
                    None => "Compress first · c to go".to_owned(),
                },
                Tone::CompressFirst,
            ),
            // **Quoted in the denominations the price is quoted in.** The core's first
            // pass answers in raw, because *"is the ore there at all"* is a question
            // with no denomination — but the pane the player is reading prices the same
            // purchase as `1 Compressed Stone`, and a toast saying `100 Stone` under it
            // reads as a second, larger price. `denominations` re-splits it by
            // `CostLine`'s own rule; `held` goes through the same call so the two
            // numbers stay comparable.
            //
            // The material is named from `item.material()` rather than by printing the
            // `Item`: the denomination now lives in the numbers, so `Item`'s `Display`
            // would name it twice the day the core's first pass stops returning a raw
            // one.
            Affordability::Insufficient(shortfalls) => (
                match shortfalls.first() {
                    Some(Shortfall { item, needed, held }) => format!(
                        "Not enough {} — {} needed, {} held",
                        item.material().name(),
                        denominations(*needed),
                        denominations(*held)
                    ),
                    None => "Not enough ore".to_owned(),
                },
                Tone::Refusal,
            ),
        };
        self.toasts.push(message, tone, TOAST_TTL);
    }

    /// Toasts whatever a core purchase refused with, verbatim.
    ///
    /// **Verbatim, and that is the point of [`CoreError`]'s own wording**: a maxed
    /// track, a spent pickaxe and an unvisited mine each say what they are, and a
    /// front-end re-phrasing them would be a second copy of the rule. An `Ok` here
    /// means the retry succeeded, which cannot happen — the caller only asks after a
    /// count of zero — so it is silent rather than announcing a purchase twice.
    ///
    /// [`CoreError`]: skylode_core::error::CoreError
    fn announce_core_refusal(&mut self, outcome: Result<(), skylode_core::error::CoreError>) {
        if let Err(refusal) = outcome {
            self.toasts
                .push(refusal.to_string(), Tone::Refusal, TOAST_TTL);
        }
    }

    /// Announces a refused purchase in the price's words when there is a price, and in
    /// the core's when there is not.
    ///
    /// **One sentence for all four purchase doors**, which they did not have: the
    /// pickaxe chain read [`announce_refusal`](App::announce_refusal) while the enchant
    /// and mine tracks fell through to [`CoreError`]'s own `Display` — a different
    /// shape (`need 1000 Compressed Emerald, have 0`), and one that never passes through
    /// [`grouped`], so the longest numbers in the game were the only ones printed
    /// without a separator.
    ///
    /// **The second arm is load-bearing and stays.** `EnchantCapped`, `PickaxeMaxed`
    /// and `MineNotEntered` are not price refusals: there is no shortfall to word, and
    /// the core already says what each of them is. Re-phrasing those here would be a
    /// second copy of the rule, which is exactly what
    /// [`announce_core_refusal`](App::announce_core_refusal) exists to avoid. So the
    /// price's wording is preferred only when a price exists *and* is what was refused.
    ///
    /// [`CoreError`]: skylode_core::error::CoreError
    /// [`grouped`]: crate::format::grouped
    fn announce_purchase_refusal(
        &mut self,
        outcome: Result<(), skylode_core::error::CoreError>,
        cost: Option<&economy::Cost>,
    ) {
        let verdict =
            cost.map(|cost| economy::affordability(self.state.player().get_inventory(), cost));
        match verdict {
            Some(verdict) if verdict != Affordability::Affordable => {
                self.announce_refusal(&verdict);
            }
            // Affordable and still refused means the refusal was about something other
            // than the purse — a cap, a gate — and the core is the one holding that word.
            _ => self.announce_core_refusal(outcome),
        }
    }

    /// Remembers a refusal the Inventory screen can help with, and forgets any other.
    ///
    /// **Asked of the price rather than read off the refusal**, and that is forced
    /// twice over. [`InsufficientItems`](skylode_core::error::CoreError::InsufficientItems)
    /// is one variant for both branches —
    /// deliberately, since from the till they are the same event — so which loop the
    /// player should run is a question only [`economy::affordability`] answers. And the
    /// panel prints the *whole* price in that material (`6 Compressed + 50`), which a
    /// shortfall does not carry: it says what is missing, not what was asked for.
    ///
    /// The line is looked up by material in the `Cost` that was refused, so what the
    /// Inventory screen shows is the same [`CostLine`](skylode_core::economy::CostLine)
    /// the Upgrades pane quoted — one number, two screens, no arithmetic in between.
    ///
    /// **Every other outcome clears it**, including a plain `Insufficient`: a note that
    /// outlived its refusal would send the player to compress for a purchase they can
    /// no longer be short of in that way.
    fn remember_refusal(&mut self, purchase: &str, cost: &economy::Cost) {
        let verdict = economy::affordability(self.state.player().get_inventory(), cost);
        self.refused = match verdict {
            Affordability::CompressFirst(shortfalls) => shortfalls
                .first()
                .and_then(|shortfall| {
                    let material = shortfall.item.material();
                    cost.lines().iter().find(|line| line.material == material)
                })
                .map(|needed| CompressHint {
                    purchase: purchase.to_owned(),
                    needed: *needed,
                }),
            _ => None,
        };
    }

    /// Moves whichever list the open screen owns by one row.
    ///
    /// **The screen is chosen here and not in [`crate::keymap`], which is the whole shape of
    /// [`Action`]'s list gestures.** `↑` decodes to [`Action::CursorUp`] without
    /// knowing what it will move, because the keymap has no access to the run; which
    /// cursor that is lands where the state is, and that is here.
    ///
    /// Every arm delegates to [`cursor::step_in`] or [`cursor::step_index`], so *every
    /// list wraps* has one implementation rather than one per screen. A screen with no
    /// list does nothing, which is why this is a `match` with a catch-all rather than a
    /// chain of `if`s that each has to remember to be exclusive.
    fn step_list_cursor(&mut self, delta: isize) {
        match self.screen {
            Screen::Mines => {
                self.cursors.mine = cursor::step_in(&MineKind::ALL, self.cursors.mine, delta);
            }
            Screen::Inventory => {
                self.cursors.material =
                    cursor::step_in(&Material::ALL, self.cursors.material, delta);
            }
            // One screen, three lists — which is why the sub-tab is a cursor and not a
            // screen of its own: the gesture is the same `↑`, and what it moves is
            // whatever is showing.
            Screen::Upgrades => match self.cursors.upgrade_tab {
                UpgradeTab::Pickaxe => {
                    self.cursors.pickaxe_rung = cursor::step_index(
                        upgrade::ladder().len(),
                        self.cursors.pickaxe_rung,
                        delta,
                    );
                }
                UpgradeTab::Enchants => {
                    self.cursors.enchant =
                        cursor::step_in(&cursor::enchant_tracks(), self.cursors.enchant, delta);
                }
                UpgradeTab::Mines => {
                    self.cursors.mine_track =
                        cursor::step_in(&cursor::mine_tracks(), self.cursors.mine_track, delta);
                }
            },
            // The ladder is `1..=LEVEL_CAP`, contiguous and one-based, so the step is
            // over indices and the level is that index plus one. Routed through
            // `step_index` rather than done here with a `+ delta`, so the wrap has one
            // implementation on this screen too rather than a second `%` that could
            // disagree with it about which end comes after the last row.
            Screen::Levels => {
                let index = cursor::step_index(LEVEL_CAP as usize, self.level_index(), delta);
                self.cursors.level = index.saturating_add(1) as u32;
            }
            // **The history is a list, so it wraps**, on `docs/UI.md` §9's own test:
            // what stops at its ends is the dial, the spinner and the dip caret, and
            // none of those is a list. Its length is read off the buffer rather than
            // carried, because the buffer is the only thing that knows — the reducer
            // has no view and no geometry.
            //
            // An empty log answers `0` from `step_index` and the panel draws nothing,
            // so the first announcement of a session finds the cursor already on it.
            Screen::Stats => {
                self.cursors.history =
                    cursor::step_index(self.toasts.len(), self.cursors.history, delta);
            }
            _ => {}
        }
    }

    /// The Levels cursor as an index into the roadmap's rows.
    ///
    /// Levels are one-based and lists are not, and this is the one place the offset is
    /// written — a second `- 1` somewhere else is how a cursor ends up one row from
    /// where it is drawn.
    fn level_index(&self) -> usize {
        (self.cursors.level as usize).saturating_sub(1)
    }

    /// Collects the reward under the Levels cursor, and says what it handed over.
    ///
    /// **The refusal is announced, unlike the richness dial's.** Reaching the end of a
    /// slider is not a player error and toasting it would bury the announcements that
    /// matter; pressing `Enter` on a row with nothing on it is a different case, since
    /// the row's own mark column says whether there is anything there and the player
    /// has read it or not. One toast per press, and no press repeats.
    fn claim_at_cursor(&mut self) {
        let level = self.cursors.level;
        match self.state.claim_level(level) {
            Ok(reward) => {
                let message = format!("Claimed Lv {level} — {}", announce::payout(&reward.payout));
                self.toasts.push(message, Tone::Success, TOAST_TTL);
            }
            Err(refusal) => self
                .toasts
                .push(refusal.to_string(), Tone::Neutral, TOAST_TTL),
        }
    }

    /// Collects everything waiting, and announces the sweep rather than each level.
    ///
    /// **One toast for the lump**, which is the same argument
    /// [`announce_refusal`](App::announce_refusal) makes for naming only the first
    /// shortfall: a player who has just crossed six levels offline gets six three-second
    /// toasts stacked on top of each other and reads none of them. What they need to
    /// know is that the sweep happened and how much of it there was; the ladder behind
    /// the toast still lists every row, now unmarked.
    ///
    /// A sweep that collects nothing is neutral rather than a refusal: the key is only
    /// advertised when something is waiting, so this is a press against an empty queue
    /// and not a mistake about a particular level.
    fn claim_everything(&mut self) {
        let collected = self.state.claim_all();
        // Matched on the **slice** and not on its length, which is what makes the
        // one-level arm bind the pair directly. A `match len { 1 => collected.first()
        // … }` needs an arm for a `None` the length has already ruled out — a branch
        // no test can reach and no reader can verify. Slice patterns are how Rust
        // lets the compiler carry that instead.
        let message = match collected.as_slice() {
            [] => "Nothing waiting to claim".to_owned(),
            [(level, reward)] => {
                format!("Claimed Lv {level} — {}", announce::payout(&reward.payout))
            }
            many => format!("Claimed {} levels", many.len()),
        };
        let tone = if collected.is_empty() {
            Tone::Neutral
        } else {
            Tone::Success
        };
        self.toasts.push(message, tone, TOAST_TTL);
    }

    /// Slides the selected mine's richness dial one step, silently at its bounds.
    ///
    /// **The upper bound is the core's refusal, and it is deliberately dropped.**
    /// The obvious alternative — read the ceiling, clamp here, and only call when
    /// the step is legal — puts a second copy of "a dial may not pass its bought
    /// ceiling" in the front-end, where it can fall out of step with the one rule
    /// that matters. Asking and being told no is how this finds the edge.
    ///
    /// Dropping the answer is the part that needs justifying, since nothing else in
    /// this crate discards a [`Result`]. A player holding `→` at the ceiling has not
    /// made a mistake; they have reached the end of the slider, and the bar visibly
    /// stops. A toast per repeat of a held key would bury the announcements that
    /// matter under one the player can already see. Every *other* refusal on this
    /// screen — a locked mine — still toasts, because that one the player cannot see.
    ///
    /// The lower bound has no refusal to lean on, because there is nothing below
    /// zero for the core to object to, so it is a saturating subtraction here.
    ///
    /// A mine this run has never entered reads as dial 0 and is refused above it,
    /// which is what it would be created at — so the arrows do nothing there,
    /// correctly, and without a special case.
    fn step_richness_dial(&mut self, delta: i32) {
        let kind = self.cursors.mine;
        let setting = self
            .state
            .mine(kind)
            .map_or(0, Mine::get_richness_setting)
            .saturating_add_signed(delta);
        let _ = self.state.set_mine_richness_setting(kind, setting);
    }

    /// Enters the selected mine, or says why it will not open.
    ///
    /// The jump to the Mine screen is the **only screen-to-screen edge** in
    /// UI-EN.md §6.1's graph, and it is worth the exception: choosing a mine and then
    /// having to press `1` to go look at it is a chore with no decision in it.
    ///
    /// A refusal becomes a toast rather than a modal because
    /// [`CoreError`]'s own wording already names both
    /// axes — *"the End mine needs level 30 and a Netherite pickaxe"* — and the player
    /// is looking at the row that says so. The screen does not change, which is the
    /// other half of the answer.
    fn enter_selected_mine(&mut self) {
        match self.state.select_mine(self.cursors.mine) {
            Ok(()) => self.screen = Screen::Mine,
            Err(refusal) => {
                self.toasts
                    .push(refusal.to_string(), Tone::Refusal, TOAST_TTL);
            }
        }
    }

    /// Runs whatever the clock says is due: the simulation steps, then their fallout.
    ///
    /// **`now` is a parameter, and that is the same rule the core keeps one level
    /// down.** A function that reads `Instant::now()` itself cannot be told what time
    /// it is, so every property worth asserting here — a step ran, three steps ran,
    /// an hour of arrears was dropped rather than replayed, the hold window expired —
    /// would be a test of the machine's speed. The loop reads the clock; this reads
    /// the argument.
    ///
    /// The order is not interchangeable:
    ///
    /// 1. **Fold the keyboard edge**, so a `Space` pressed microseconds ago is held
    ///    for the step it belongs to rather than the next one.
    /// 2. **Answer `space_held` once**, not once per step. The window is measured
    ///    against `now`, and the steps below may be arrears — asking again inside the
    ///    loop would be asking about an instant that has not happened.
    /// 3. **Run the steps due**, capped.
    /// 4. **Say what they did**, then expire what has been said long enough.
    ///
    /// **It answers *whether the screen owes a redraw*** rather than writing that
    /// answer somewhere. The redraw policy belongs to
    /// [`Session`](crate::session::Session) — it is the one that knows a splash screen
    /// is up, or that the ceiling has not passed — so the run reports what it did and
    /// the loop decides what to do about it. A step that ran changed something by
    /// construction: the auto-miner credits on every one.
    pub(crate) fn advance(&mut self, now: Instant) -> bool {
        match self.mine_key_edge.take() {
            Some(MineKeyEdge::Down) => self.last_mine_key = Some(now),
            // Not "stop the current step": the field *is* the answer, so clearing it
            // is the whole of stopping. Under a terminal that cannot report this, the
            // window below reaches the same state a little later.
            Some(MineKeyEdge::Up) => self.last_mine_key = None,
            None => {}
        }

        let input = Input {
            space_held: self
                .last_mine_key
                .is_some_and(|last| now.duration_since(last) < HOLD_WINDOW),
        };

        let mut events = Vec::new();
        let mut steps = 0;
        while now >= self.next_tick && steps < MAX_CATCHUP_TICKS {
            events.extend(self.state.tick(input));
            self.next_tick += SIM_PERIOD;
            steps += 1;
        }
        // Arrears past the cap are dropped rather than owed: a session that was
        // suspended comes back to a deadline in the future instead of an hour of
        // ticks to replay. What the player is owed for that hour is the offline
        // accrual's answer, and it is a multiplication rather than a replay.
        if steps == MAX_CATCHUP_TICKS {
            self.next_tick = now + SIM_PERIOD;
        }

        for event in &events {
            let (text, tone) = announce::of(event);
            // `push_at` and not `push`: a toast raised by a step must expire three
            // seconds after the instant that step ran, and the prune two lines below
            // is measured against that same `now`. Reading the clock twice inside one
            // `advance` is how a toast gets pruned in the same breath it is raised.
            self.toasts.push_at(text, tone, TOAST_TTL, now);

            // **The same event, consumed twice and independently** (UI.md §7). The toast
            // above says *what* fired; this says *where*, and neither waits for the
            // other — three seconds and two hundred milliseconds have nothing to say to
            // each other. `cells` and not `broken`: the shape deliberately covers ground
            // the swing had already cleared, and a blast the player watches has to look
            // like a blast rather than like the four cells that happened to be left
            // standing.
            //
            // The mine is read from the run rather than carried on the event, because
            // the event is about an enchant and not about a grid — and the only mine a
            // proc can fire in is the one the player is standing in, which
            // `current_mine()` answers totally.
            if let GameEvent::SpatialProc { cells, .. } = event {
                self.flash
                    .push(self.state.current_mine().kind(), cells, now);
            }
        }

        // A step that ran changed something by construction — the auto-miner credits
        // on every one — so this is `steps > 0` rather than a test of the events,
        // which most steps produce none of.
        steps > 0
    }

    /// Paints one frame: tab bar, active screen, then the overlays on top.
    ///
    /// Order is the layering: overlays draw last precisely so they cover the
    /// screen rather than being covered by it.
    ///
    /// `pub(crate)` for [`sync_view`](App::sync_view)'s reason: the loop calling it
    /// lives in [`Session`](crate::session::Session) now. It stays `&self` — a pure
    /// read — which is what lets a test draw an `App` it does not own.
    pub(crate) fn render(&self, frame: &mut Frame, now: Instant) {
        let area = frame.area();

        // The terminal-too-small filter used to stand here and now stands one level
        // up, in `Session::render` (UI-EN.md §6.2). It is not a screen and not a
        // modal: below the 80×24 budget it replaces the whole frame regardless of
        // what is up — *including the title*, which is what this function cannot
        // see. Keeping a second copy here would mean a check that can never be true.

        // Everything below draws into the *band*, not into the terminal. The filter
        // above deliberately still reads the whole frame — "is the window big
        // enough" is a question about the window — but from here on `area` is the
        // interface, and a toast handed the terminal instead would centre itself
        // over the margin rather than over the screen it is announcing about.
        //
        // Below the caps `Max` is satisfied by the whole width and `Flex::Center`
        // has nothing to centre, so at 80×24 this is the identity and the counted
        // frames are untouched.
        let area = area.centered(Constraint::Max(MAX_WIDTH), Constraint::Max(MAX_HEIGHT));

        // The tab bar takes exactly one row and the screen takes the rest —
        // "the grid is fixed, the chrome flexes" starts here.
        let [tabs_area, body_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);

        self.render_tabs(frame, tabs_area);
        self.screen.render(frame, body_area, &self.view);

        // Overlays, outermost last. A modal draws over the whole frame, including
        // the toasts — it captured the input that would dismiss them, so it owns the
        // surface until it closes.
        self.toasts.render(frame, area, now);
        // Borrowed rather than copied: `render` takes `&self` and the confirm carries
        // a `String` it only ever reads.
        if let Some(modal) = &self.modal {
            match modal {
                // Help reports the bindings of the screen it was opened over, so it
                // is handed the current screen and the config the sub-tab line reads.
                Modal::Help => help::render(frame, area, self.screen, &self.config),
                // The dialog reads the run rather than the `View`: it is about a pile
                // as it stands *now*, and the snapshot behind it was projected before
                // the conversion the player is about to confirm.
                Modal::Compress {
                    material,
                    direction,
                    units,
                } => compression::render(
                    frame,
                    area,
                    self.state.player().get_inventory(),
                    *material,
                    *direction,
                    *units,
                ),
                // The opposite choice to the dialog above, and for the opposite reason:
                // the dip is about a purchase whose numbers the player has *already
                // read* in the pane behind, so it draws that same projection rather
                // than a second reading of the run that could disagree with it.
                Modal::Dip { buy, .. } => {
                    if let UpgradeDetail::Pickaxe(detail) = &self.view.upgrades.pickaxe.detail {
                        dip::render(frame, area, detail, *buy);
                    }
                }
                // Both prestige boxes read the projection the Stats panel behind them
                // draws from, which is the dip modal's rule and is what stops the box
                // quoting a price the panel disagrees with.
                Modal::PrestigePreview => {
                    prestige_overlay::render_preview(frame, area, &self.view.prestige);
                }
                Modal::PrestigeConfirm { typed } => {
                    prestige_overlay::render_confirm(frame, area, &self.view.prestige, typed);
                }
                // Draws from `dev` and nothing else — the menu is about its own dialled
                // values, not about the run behind it, so there is no projection here
                // for it to disagree with.
                #[cfg(debug_assertions)]
                Modal::Dev => {
                    if let Some(dev) = &self.dev {
                        dev::render(frame, area, dev);
                    }
                }
            }
        }
    }

    /// Draws the ring as a `Tabs` widget, numbered for the `1`..`6` shortcuts.
    ///
    /// The digits are printed rather than merely bound so the shortcut is
    /// discoverable without opening help — and the prestige readout that used to
    /// share this row was dropped precisely to keep six numbered tabs fitting in
    /// 80 columns (UI-EN.md §5.7.5).
    ///
    /// **Both styles are stated, though ratatui's default highlight is already
    /// reversed.** UI.md §3 requires the selected tab to be reverse video rather
    /// than bracketed, and relying on a library default to satisfy a documented
    /// requirement means a future ratatui release could change the interface
    /// without changing this crate. Adding [`theme::ACCENT`] on top is what makes
    /// the reversed block read as the same "you are here" hue the list cursor and
    /// the gauges use.
    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let titles = Screen::ALL
            .iter()
            .enumerate()
            .map(|(position, screen)| format!(" {} {} ", position + 1, screen.title()));
        let tabs = Tabs::new(titles)
            .select(self.screen.index())
            .divider("│")
            .style(Style::default().fg(theme::MUTED))
            .highlight_style(
                Style::default()
                    .fg(theme::ACCENT)
                    .add_modifier(Modifier::REVERSED),
            );
        frame.render_widget(tabs, area);
        #[cfg(debug_assertions)]
        self.render_dev_marker(frame, area);
    }

    /// Stamps [`dev::MARKER`] at the right end of the tab row, coloured by the mode.
    ///
    /// **On the tab row and not in a footer**, because it has to be true on every
    /// screen: what the marker warns about is that a price on the Upgrades screen is not
    /// what will be charged, and a footer note there would be invisible from the Mine
    /// screen the player switches back to.
    ///
    /// Drawn *after* the `Tabs` widget and over its right edge, **into a rectangle of
    /// its own** — the last [`MARKER_COLUMNS`] of the row. That is the difference
    /// between a bound that is documented and one that is enforced: handed the whole
    /// row, a right-aligned marker one character too long silently eats the end of
    /// `6 Levels`, which is precisely what the first draft's `DEV FREE` did. Clipped to
    /// its own rect it can only ever lose its own last letter, and the test in
    /// [`dev`] keeps it from having to.
    ///
    /// A narrower terminal is [`too_small`]'s business and never reaches here, so the
    /// subtraction below cannot underflow — `saturating_sub` regardless, because a
    /// panicking layout is never the right way to report a small window.
    ///
    /// [`MARKER_COLUMNS`]: crate::overlay::dev::MARKER_COLUMNS
    #[cfg(debug_assertions)]
    fn render_dev_marker(&self, frame: &mut Frame, area: Rect) {
        let Some(state) = &self.dev else {
            return;
        };
        // Refusal red for the mode that changes what a purchase costs, muted for the
        // one that only says a key exists: `theme` already owns which of those two a
        // reader should look at, and three columns is all the tab row leaves for saying
        // it in words.
        let colour = if state.free_upgrades {
            theme::REFUSED
        } else {
            theme::MUTED
        };
        let width = u16::try_from(dev::MARKER_COLUMNS)
            .unwrap_or(0)
            .min(area.width);
        let corner = Rect {
            x: area.x + area.width.saturating_sub(width),
            width,
            ..area
        };
        frame.render_widget(
            Paragraph::new(dev::MARKER)
                .right_aligned()
                .style(Style::default().fg(colour)),
            corner,
        );
    }
}

#[cfg(test)]
mod tests {

    use ratatui::{
        Terminal,
        backend::TestBackend,
        buffer::Buffer,
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
        style::Color,
    };
    use skylode_core::{game::Input, pickaxe::PickaxeTier, save};

    use super::*;
    // Both belong to the loop, which lives in `session` now — so they are imported
    // here rather than at the top of the file, where a release build would find them
    // unused.
    use crate::{keymap, palette};

    /// The seed every test session starts from.
    ///
    /// Any value would do; what matters is that it is *fixed*. `GameState::new`
    /// draws the opening mine's whole grid from it, so a seed off the clock would
    /// hand each run of the suite a different picture and make "did anything get
    /// painted" the strongest assertion anyone could write about the grid.
    const SEED: u64 = 0x5B1_0DE;

    /// A session over a fixed run — what every test below opens with.
    ///
    /// `UNIX_EPOCH` as `now` for the seed's reason: it is the offline accrual's
    /// reference point, and phase 7's `resume` credits the span since it. A test
    /// that read the clock would be measuring how long ago the file was written.
    pub(super) fn session() -> App {
        App::new(GameState::new(SEED, std::time::UNIX_EPOCH))
    }

    /// Draws `app` into an off-screen 80×24 terminal and hands back the cells.
    ///
    /// `TestBackend` is what makes the render path testable at all: no tty, no
    /// raw mode, just a buffer we can read back — so "does the tab bar actually
    /// say `6 Levels`" is an assertion rather than something eyeballed.
    ///
    /// Its operations are typed `Result<_, Infallible>` — writing to a `Vec` of
    /// cells cannot fail — so the errors are discharged with an empty `match` on
    /// the uninhabited error rather than an `unwrap` the lints would flag. Same
    /// trick as the `Modal` slot above: no value can exist, so there is no arm.
    pub(super) fn render_to_buffer(app: &App) -> Buffer {
        render_to_sized_buffer(app, 80, 24)
    }

    /// The same, at an instant of the test's choosing — for the one thing on a frame
    /// that depends on what time it is, which is whether a toast is still showing.
    fn render_at(app: &App, now: Instant) -> Buffer {
        render_to_sized_buffer_at(app, 80, 24, now)
    }

    /// The same, at an arbitrary size — for the too-small filter, whose whole job
    /// is what happens below 80×24.
    fn render_to_sized_buffer(app: &App, width: u16, height: u16) -> Buffer {
        render_to_sized_buffer_at(app, width, height, Instant::now())
    }

    /// Both axes at once. The two helpers above each fix one of them, because a call
    /// site that had to name a size *and* an instant would say neither clearly.
    pub(super) fn render_to_sized_buffer_at(
        app: &App,
        width: u16,
        height: u16,
        now: Instant,
    ) -> Buffer {
        let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        if let Err(infallible) = terminal.draw(|frame| app.render(frame, now)) {
            match infallible {}
        }
        terminal.backend().buffer().clone()
    }

    /// The text of row `y`, joined back into a string.
    fn row(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect()
    }

    /// Every row of the frame, joined — for "is this text on screen anywhere".
    pub(super) fn whole_frame(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| row(buffer, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The `(left, right)` columns of the first full-width bordered box drawn.
    ///
    /// The band's edges are read off a `panel`'s own corners rather than off "the
    /// first non-blank cell": the tab bar pads its labels, so row 0's ink starts a
    /// couple of columns in and would report a band narrower than it is. The Mine
    /// screen's Haul strip spans the whole body, so its `╭` and `╮` *are* the two
    /// edges — which is the measurement the assertions below actually want.
    fn box_span(buffer: &Buffer) -> Option<(u16, u16)> {
        let cells = |glyph: &'static str| {
            (0..buffer.area.height)
                .flat_map(move |y| (0..buffer.area.width).map(move |x| (x, y)))
                .find(|position| buffer[*position].symbol() == glyph)
                .map(|(x, _)| x)
        };
        Some((cells("╭")?, cells("╮")?))
    }

    #[test]
    fn at_the_counted_size_the_band_is_the_whole_terminal() {
        // The identity case, and the one that matters most: `Max` above the actual
        // width leaves `Flex::Center` nothing to centre, so 80×24 is untouched and
        // every counted wireframe in UI-EN.md §5 still describes what is drawn.
        let buffer = render_to_buffer(&session());
        assert_eq!(box_span(&buffer), Some((0, 79)));
    }

    #[test]
    fn a_terminal_past_the_caps_is_a_centred_band_with_bare_margins() {
        // 250×80, capped to 160×48: the interface is 160 wide however much room it
        // is given, and the leftover 90 columns become equal margins.
        let buffer = render_to_sized_buffer(&session(), 250, 80);
        let margin = (250 - MAX_WIDTH) / 2;
        assert_eq!(
            box_span(&buffer),
            Some((margin, margin + MAX_WIDTH - 1)),
            "the band is not a centred {MAX_WIDTH} columns"
        );

        // The margin is genuinely untouched, not merely dark: a background painted
        // out to the edges would look identical to a reader and would be no margin.
        for y in 0..80 {
            for x in 0..margin {
                assert_eq!(buffer[(x, y)].symbol(), " ", "ink at ({x}, {y})");
            }
        }
    }

    #[test]
    fn between_the_minimum_and_the_caps_the_interface_takes_the_whole_terminal() {
        // The band only bites past the caps. At 120×40 — a very ordinary window —
        // nothing is centred and no column is wasted.
        let buffer = render_to_sized_buffer(&session(), 120, 40);
        assert_eq!(box_span(&buffer), Some((0, 119)));
    }

    #[test]
    fn a_new_session_opens_on_the_mine_tab_with_nothing_stacked() {
        let app = session();
        assert_eq!(app.screen, Screen::Mine);
        assert!(app.modal.is_none());
        assert!(app.leaving.is_none());
    }

    #[test]
    fn next_and_prev_walk_the_ring() {
        let mut app = session();
        app.update(Action::NextScreen);
        assert_eq!(app.screen, Screen::Mines);
        app.update(Action::PrevScreen);
        assert_eq!(app.screen, Screen::Mine);
    }

    #[test]
    fn the_ring_wraps_in_both_directions() {
        let mut app = session();
        app.update(Action::PrevScreen);
        assert_eq!(app.screen, Screen::Levels);
        app.update(Action::NextScreen);
        assert_eq!(app.screen, Screen::Mine);
    }

    #[test]
    fn selecting_a_tab_jumps_straight_to_it() {
        let mut app = session();
        app.update(Action::SelectScreen(3));
        assert_eq!(app.screen, Screen::Upgrades);
    }

    #[test]
    fn selecting_a_tab_that_does_not_exist_leaves_the_screen_alone() {
        let mut app = session();
        app.update(Action::SelectScreen(99));
        assert_eq!(app.screen, Screen::Mine);
    }

    #[test]
    fn quit_raises_the_flag_the_loop_watches() {
        let mut app = session();
        app.update(Action::ToTitle);
        assert_eq!(app.leaving, Some(Leaving::ToTitle));

        // And `Ctrl-C`'s own action is the other exit, not a louder version of the
        // same one: the session ends the process for one and rebuilds the title for
        // the other.
        let mut app = session();
        app.update(Action::Quit);
        assert_eq!(app.leaving, Some(Leaving::Process));
    }

    #[test]
    fn the_mine_key_records_an_edge_and_nothing_else() {
        // **`update` has no clock, and this is what that costs and buys.** The reducer
        // cannot answer "is the player mining" — that is a question about time — so it
        // records only which way the key went and leaves the stamping to `advance`.
        // The payoff is that every other test of `update` stays a pure transition.
        let mut app = session();
        assert_eq!(app.mine_key_edge, None);
        assert_eq!(app.last_mine_key, None);

        app.update(Action::MinePressed);
        assert_eq!(app.mine_key_edge, Some(MineKeyEdge::Down));
        assert_eq!(app.last_mine_key, None, "the reducer read a clock");

        app.update(Action::MineReleased);
        assert_eq!(app.mine_key_edge, Some(MineKeyEdge::Up));
    }

    #[test]
    fn the_tab_bar_shows_all_six_tabs_with_their_digits() {
        let buffer = render_to_buffer(&session());
        let bar = row(&buffer, 0);
        for (position, screen) in Screen::ALL.iter().enumerate() {
            let label = format!("{} {}", position + 1, screen.title());
            assert!(
                bar.contains(&label),
                "tab bar is missing {label:?}: {bar:?}"
            );
        }
    }

    #[test]
    fn every_tab_draws_itself_and_stays_selected() {
        // Walks the whole ring through the renderer. A screen that panics on an
        // empty area, or that forgets to mark itself selected, fails here rather
        // than the first time someone presses its digit.
        for (position, screen) in Screen::ALL.iter().enumerate() {
            let mut app = session();
            app.update(Action::SelectScreen(position));
            assert_eq!(app.screen, *screen);

            let buffer = render_to_buffer(&app);
            let frame = whole_frame(&buffer);
            assert!(
                frame.contains(screen.title()),
                "{} did not draw its own title:\n{frame}",
                screen.title()
            );
        }
    }

    #[test]
    fn the_tab_bar_fits_the_eighty_column_reference_width() {
        // UI-EN.md §5.7.5 counts the six-tab bar at 65 columns, which is what
        // dropping the prestige readout bought. If a renamed tab ever pushes it
        // past 80 this fails rather than silently truncating on a real terminal.
        let buffer = render_to_buffer(&session());
        let bar = row(&buffer, 0);
        assert!(bar.trim_end().chars().count() <= 80);
    }

    #[test]
    fn the_selected_screen_is_the_one_drawn() {
        let mut app = session();
        app.update(Action::SelectScreen(2));
        let frame = whole_frame(&render_to_buffer(&app));
        // The Inventory table, drawn from the run. A fresh one has mined nothing, so
        // the assertion is on the *table* and not on a count: it lists all fifteen
        // materials whether or not the player holds any, which is the whole reason
        // the projection walks `Material::ALL` rather than the sparse map.
        assert!(frame.contains("Inventory"), "{frame}");
        assert!(frame.contains("Ancient Debris"), "{frame}");
        assert!(frame.contains("Amethyst"), "{frame}");
    }

    #[test]
    fn the_mine_tab_paints_coloured_cells() {
        // The grid is the one thing on screen that carries information in its
        // *background*, so "did anything get painted" is a real assertion here and
        // not a tautology: every other widget leaves `bg` at `Reset`.
        use ratatui::style::Color;

        let buffer = render_to_buffer(&session());
        let painted = buffer.content().iter().any(|cell| cell.bg != Color::Reset);
        assert!(
            painted,
            "the mine screen drew no swatch:\n{}",
            whole_frame(&buffer)
        );
    }

    #[test]
    fn opening_and_closing_help_stacks_then_clears_the_modal() {
        let mut app = session();
        app.update(Action::OpenHelp);
        assert_eq!(app.modal, Some(Modal::Help));
        app.update(Action::CloseModal);
        assert!(app.modal.is_none());
    }

    #[test]
    fn help_draws_over_the_screen_it_was_opened_on() {
        let mut app = session();
        app.update(Action::OpenHelp);
        let frame = whole_frame(&render_to_buffer(&app));
        // The right pane's title is Help-only, so its presence is Help on top.
        assert!(frame.contains("Reading the screen"), "{frame}");
    }

    #[test]
    fn a_toast_is_drawn_over_the_screen_underneath() {
        let mut app = session();
        app.toasts
            .push("Mine refilled".to_owned(), Tone::Neutral, TOAST_TTL);
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Mine refilled"), "{frame}");
    }

    /// A toast leaves the **screen** once its moment has passed, and stays in the log.
    ///
    /// **Expiry moved out of `advance` and into the frame.** A step used to delete the
    /// entry, which is what left the Stats history with nothing to read; the buffer now
    /// keeps everything and the drawing asks whether an announcement is still inside
    /// its window. So this is a test about two renderings of one buffer, and the
    /// instant is the test's to choose on both.
    #[test]
    fn a_toast_leaves_the_screen_once_its_moment_has_passed_and_stays_in_the_log() {
        let start = Instant::now();
        let mut app = session();
        app.toasts
            .push_at("Excavator!".to_owned(), Tone::Success, TOAST_TTL, start);

        let live = whole_frame(&render_at(&app, start + SIM_PERIOD));
        assert!(live.contains("Excavator!"), "{live}");

        let later = start + TOAST_TTL + Duration::from_millis(1);
        let expired = whole_frame(&render_at(&app, later));
        assert!(!expired.contains("Excavator!"), "{expired}");

        assert_eq!(app.toasts.len(), 1, "the log forgot what it had announced");
    }

    /// When the app's **next** simulation step falls due.
    ///
    /// **Read off the app rather than off a clock the test started**, and the
    /// difference is a real one: `App::new` anchors its first deadline on the instant
    /// it was built, which is a few microseconds *after* an `Instant::now()` the test
    /// took first — so a hand-built `start + SIM_PERIOD` lands just short of the
    /// deadline and no step runs. Asking the app when it is next due removes the race
    /// instead of padding around it.
    ///
    /// Read it **once, before advancing**: the deadline moves as steps run, so a
    /// second reading answers a different question. Later instants are built as
    /// `due + SIM_PERIOD * n` from the first.
    fn step_due(app: &App) -> Instant {
        app.next_tick
    }

    /// A frame drawn the way [`App::run`] draws one: **project, then paint, on the same
    /// instant**.
    ///
    /// [`render_at`] alone is not enough for anything time-varying inside the grid,
    /// because the [`View`] it paints is a *cache*: a toast expires in
    /// [`render`](App::render) and can therefore be moved by the instant alone, but a
    /// proc flash is resolved to its beat back in [`sync_view`](App::sync_view). The two
    /// halves of "what time is it" live in two functions and agree only because the loop
    /// hands them one `now` — so a test that wants a beat has to do the same, or it is
    /// asserting against whichever instant the projection last happened to see.
    fn render_frame_at(app: &mut App, now: Instant) -> Buffer {
        app.sync_view(now);
        render_at(app, now)
    }

    /// A session kitted out until a spatial proc is reachable by mining.
    ///
    /// **Through the front-end's own doors, and that is the point.** `Enchants::upgrade`
    /// is `pub(crate)`, so this crate cannot enchant a pickaxe directly — what it *can*
    /// do is what a player does: turn free upgrades on in the dev menu and buy the track.
    /// So the proc this reaches is one the rules produced, from the seeded generator, on
    /// a run that was played to rather than patched into place.
    ///
    /// The pickaxe ladder comes first because Explosive is priced past what the opening
    /// Stone mine drops even for free — and because a Netherite instamine is what makes
    /// two thousand swings fit in a test.
    fn blasting() -> App {
        let mut app = session().with_dev(true);
        if let Some(dev) = app.dev.as_mut() {
            dev.free_upgrades = true;
        }
        app.screen = Screen::Upgrades;
        app.cursors.upgrade_tab = UpgradeTab::Pickaxe;
        app.update(Action::BuyMax);
        app.cursors.upgrade_tab = UpgradeTab::Enchants;
        app.cursors.enchant = EnchantType::Explosive;
        app.update(Action::BuyMax);
        app.screen = Screen::Mine;
        app.toasts = Toasts::new();
        app
    }

    /// Mines until a blast fires, and hands back the instant the step that fired it ran.
    ///
    /// Returns the instant rather than the event, because the instant is what every
    /// assertion about a beat is measured from — and it is the *step's* instant, which is
    /// exactly what `Flashes` was stamped with.
    fn mine_until_a_blast(app: &mut App, start: Instant) -> Instant {
        app.update(Action::MinePressed);
        for step in 1..2_000u32 {
            let now = start + SIM_PERIOD * step;
            app.advance(now);
            if !app
                .flash
                .resolve(app.state.current_mine().kind(), now)
                .is_empty()
            {
                return now;
            }
        }
        unreachable!("a maxed Explosive never fired in 2 000 steps")
    }

    /// How many cells of `buffer` are on each beat, as `(bright, fading)`.
    ///
    /// **Found by colour and not by glyph, and that is not a preference.** `█` is also
    /// the filled symbol of all three status gauges, so a frame-wide search for it
    /// returns a hit on *every* frame whether or not anything is flashing — a test
    /// written that way passes with the feature ripped out. The same trap `screen::mine`
    /// already documents for `░`, which is the unfilled gauge and the value stipple at
    /// once. The blast colour appears nowhere else on the screen, so it is the honest
    /// discriminator here; the glyphs are pinned in `widget`'s own tests, over a grid
    /// with no chrome in it.
    fn beats_on(buffer: &Buffer) -> (usize, usize) {
        let cells =
            || (0..buffer.area.height).flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)));
        let bright = cells()
            .filter(|&(x, y)| buffer[(x, y)].bg == palette::BLAST)
            .count();
        let fading = cells()
            .filter(|&(x, y)| {
                buffer[(x, y)].fg == palette::BLAST && buffer[(x, y)].bg == Color::Reset
            })
            .count();
        (bright, fading)
    }

    /// **The wire, end to end**: a real proc puts a real blast on a real frame.
    ///
    /// Asserted on the buffer and not on the `View`'s own field, because everything
    /// between the event and the paint is what this is about — the push in
    /// [`advance`](App::advance), the resolve in [`sync_view`](App::sync_view), the
    /// projection, and the widget.
    #[test]
    fn a_spatial_proc_paints_a_blast_on_the_next_frame() {
        let mut app = blasting();
        let fired = mine_until_a_blast(&mut app, Instant::now());

        let buffer = render_frame_at(&mut app, fired);
        let (bright, fading) = beats_on(&buffer);
        assert!(bright > 0, "no blast reached the frame");
        assert_eq!(fading, 0, "the first frame was already fading");
        // Both channels arrived, which is what makes the shape survive a terminal that
        // dropped the hue.
        assert!(
            (0..buffer.area.height)
                .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
                .any(|(x, y)| buffer[(x, y)].bg == palette::BLAST
                    && buffer[(x, y)].symbol() == "█"),
            "the blast arrived as a colour with no glyph"
        );

        // And the toast said its half of it, from the same event and independently.
        // Read off the **log** and not off the frame: the step that fired the blast can
        // refill the mine in the same breath, and the overlay only ever shows the newest
        // announcement — so a frame search would be asking which of two toasts won a
        // race this test is not about.
        assert!(
            app.toasts
                .log(fired)
                .any(|(_, text)| text.contains("blocks")),
            "the flash fired without its announcement"
        );
    }

    /// **Each beat is drawn twice, and the number is the whole justification for
    /// 100 ms.**
    ///
    /// The redraw rate is the simulation's — every step raises `dirty`, because the
    /// auto-miner credits on every one — so the frames a flash is alive for land at
    /// `fired + n × SIM_PERIOD`. This walks exactly those instants rather than instants
    /// of its own choosing, which is what makes it a claim about the loop instead of a
    /// claim about arithmetic. Two frames per beat is the floor: at one, a late pass
    /// could drop a beat entirely and the fade would never be seen.
    #[test]
    fn each_beat_of_the_flash_is_drawn_on_two_frames() {
        let mut app = blasting();
        let fired = mine_until_a_blast(&mut app, Instant::now());

        let mut beats = Vec::new();
        for frame in 0..5u32 {
            let buffer = render_frame_at(&mut app, fired + SIM_PERIOD * frame);
            beats.push(match beats_on(&buffer) {
                (0, 0) => "gone",
                (0, _) => "fading",
                (_, 0) => "bright",
                (_, _) => "both",
            });
        }

        assert_eq!(beats, ["bright", "bright", "fading", "fading", "gone"]);
    }

    /// A blast does not follow the player into the next mine, and the buffer is what
    /// refuses rather than the several call sites that change one.
    ///
    /// Walked through `Enter` on the Mines screen — the door a player uses — so what is
    /// under test is the whole path and not `Flashes::resolve` a second time.
    #[test]
    fn a_blast_does_not_follow_the_player_into_the_next_mine() {
        let mut app = blasting();
        let fired = mine_until_a_blast(&mut app, Instant::now());
        assert!(
            !render_frame_at(&mut app, fired).content.is_empty() && !app.view.flash.is_empty(),
            "nothing was flashing to begin with"
        );

        app.screen = Screen::Mines;
        app.cursors.mine = MineKind::Coal;
        app.update(Action::Confirm);
        assert_eq!(
            app.state.current_mine().kind(),
            MineKind::Coal,
            "the walk was refused, so this proves nothing"
        );

        render_frame_at(&mut app, fired);
        assert!(
            app.view.flash.is_empty(),
            "a Stone blast is being painted onto the Coal mine"
        );
    }

    #[test]
    fn a_step_falls_due_once_every_twentieth_of_a_second() {
        // Nothing before the deadline, exactly one on it. The mine key is down, so a
        // step that ran is visible as break progress on the cell being dug — the
        // cheapest observable the run has, and one that needs no veteran save.
        let mut app = session();
        app.update(Action::MinePressed);
        let first = step_due(&app);

        app.advance(first - Duration::from_millis(1));
        assert_eq!(
            app.state.current_mine().break_ratio(),
            0.0,
            "a step ran before it was due"
        );

        app.advance(first);
        assert!(
            app.state.current_mine().break_ratio() > 0.0,
            "the step that fell due did not swing"
        );
    }

    #[test]
    fn a_late_wake_up_runs_every_step_it_owes() {
        // **What the accumulator is for.** A pass that arrives three periods late owes
        // three steps, and they are run rather than collapsed into one — otherwise a
        // busy machine would mine more slowly than an idle one, and the 20 tps rate
        // would be a description of the hardware.
        let mut punctual = session();
        let mut late = session();
        punctual.update(Action::MinePressed);
        late.update(Action::MinePressed);

        let first = step_due(&punctual);
        for step in 0..3 {
            punctual.advance(first + SIM_PERIOD * step);
        }
        late.advance(step_due(&late) + SIM_PERIOD * 2);

        assert_eq!(
            late.state.current_mine().break_ratio(),
            punctual.state.current_mine().break_ratio(),
            "the late pass did not catch up"
        );
    }

    #[test]
    fn a_suspended_session_resynchronises_instead_of_replaying_the_arrears() {
        // A closed laptop hands the loop an hour of deadlines. Replaying them would
        // freeze the interface computing what the offline accrual computes with one
        // multiplication, so the surplus is dropped and the clock re-anchored on
        // `now` — the next pass must be due one period later, not one hour ago.
        let start = Instant::now();
        let mut app = session();
        let wake = start + Duration::from_secs(3_600);

        app.advance(wake);

        assert_eq!(app.next_tick, wake + SIM_PERIOD);
    }

    #[test]
    fn the_mine_key_stays_held_until_its_window_closes() {
        // Layer 2, the one that runs on every terminal that cannot report a release:
        // the key is "still down" for as long as an auto-repeat could plausibly be on
        // its way, and up the moment that stops being true.
        //
        // The window is anchored on the *first `advance`*, not on the `update` — the
        // reducer has no clock — so both instants below are measured from there.
        let mut app = session();
        app.update(Action::MinePressed);
        let pressed = step_due(&app);

        app.advance(pressed);
        app.advance(pressed + HOLD_WINDOW - Duration::from_millis(1));
        assert!(
            app.state.current_mine().break_ratio() > 0.0,
            "the key was dropped inside its own window"
        );

        // Zero rather than "unchanged", because the core now *forfeits* the block in
        // progress on a released tick: a swing that had outlived the window would
        // leave this rising instead.
        app.advance(pressed + HOLD_WINDOW + SIM_PERIOD);
        assert_eq!(
            app.state.current_mine().break_ratio(),
            0.0,
            "the swing outlived the hold window"
        );
    }

    #[test]
    fn a_release_cuts_the_window_early_rather_than_taking_a_second_path() {
        // Layer 1. A terminal speaking the kitty protocol reports the release, and all
        // it does is reach the same state the window would have reached on its own —
        // sooner. Nothing downstream branches on which terminal this is.
        let mut app = session();
        app.update(Action::MinePressed);
        app.advance(step_due(&app));
        assert!(app.state.current_mine().break_ratio() > 0.0);

        app.update(Action::MineReleased);
        app.advance(step_due(&app));

        assert_eq!(app.last_mine_key, None);
        // Same probe as the window's own test, and the same reason: the step that ran
        // after the release was an idle one, and an idle step drops the progress it is
        // no longer earning.
        assert_eq!(
            app.state.current_mine().break_ratio(),
            0.0,
            "the pickaxe swung after the key came up"
        );
    }

    #[test]
    fn a_step_with_the_key_up_still_pays_the_auto_miner() {
        // The idle half of the tick, and the reason `dirty` is raised by *any* step
        // rather than by the events one produced: nothing is dug, and the inventory
        // still moves, so a loop that only redrew on events would show a stale haul.
        let mut app = session();

        // Enough steps for the auto-miner's carry to cross a whole block.
        let start = step_due(&app);
        for step in 0..200 {
            app.advance(start + SIM_PERIOD * step);
        }

        assert_eq!(
            app.state.current_mine().break_ratio(),
            0.0,
            "the pickaxe swung with the key up"
        );
        assert!(
            app.state
                .player()
                .get_inventory()
                .raw_value(Material::Stone)
                > 0,
            "ten seconds of auto-mining credited nothing"
        );
    }

    #[test]
    fn a_pass_with_nothing_due_asks_the_terminal_for_no_frame() {
        // The whole of "redraw on change" that the front-end can answer cheaply. A
        // step always changes something, so this only ever fires between two steps —
        // which is precisely where the ~30 fps ceiling matters once the proc flash
        // animates there. Asserted on `advance`'s answer rather than on a flag, since
        // the flag is the loop's and this is the run's half of the exchange.
        let mut app = session();
        let first = step_due(&app);

        assert!(
            !app.advance(first - Duration::from_millis(1)),
            "a pass with no step due asked for a frame"
        );
        assert!(app.advance(first), "a step ran without asking for a frame");
    }

    #[test]
    fn a_tick_that_makes_news_raises_a_toast_saying_so() {
        // The wire from `Vec<GameEvent>` to the overlay, end to end. An instamining
        // pickaxe empties the starter grid in a couple of hundred swings, and the
        // refill is the announcement that costs the least to provoke.
        let mut app = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Netherite""#),
            (r#""enchants":{}"#, r#""enchants":{"Efficiency":15}"#),
        ]);
        app.update(Action::MinePressed);
        let start = step_due(&app);

        for step in 0..1_000 {
            app.advance(start + SIM_PERIOD * step);
            // The key is re-pressed as auto-repeat would, or the window would close
            // 22 steps in and the grid would never empty.
            app.update(Action::MinePressed);
            if !app.toasts.is_empty() {
                break;
            }
        }

        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Mine refilled"), "{frame}");
    }

    #[test]
    fn a_session_opens_on_the_run_it_was_handed() {
        // `Default` used to live here — clippy asks for one beside an argument-less
        // `new` — and went with the argument: there is no default run, because there
        // is no default seed a front-end could invent without reading a clock.
        //
        // What replaces it is the assertion that actually matters now: the view the
        // session opens with describes the state it was given, and not the fixture.
        // `GameState::new` starts every run in the Stone mine at level 1.
        let app = session();
        assert_eq!(app.view.mine_name, "Stone Mine");
        assert_eq!(app.view.player_level, 1);
        assert_eq!(app.view.mine_kind, app.state.current_mine().kind());
    }

    #[test]
    fn no_toast_means_nothing_is_overlaid() {
        let plain = whole_frame(&render_to_buffer(&session()));

        let mut app = session();
        app.toasts
            .push("Mine refilled".to_owned(), Tone::Neutral, TOAST_TTL);
        let toasted = whole_frame(&render_to_buffer(&app));

        // The toast borrows cells for a frame; without one the frame is untouched.
        assert_ne!(plain, toasted);
    }

    /// A session on the Mines tab — where the list gestures are answered.
    fn browsing_mines() -> App {
        let mut app = session();
        app.screen = Screen::Mines;
        app
    }

    #[test]
    fn a_fresh_session_points_at_the_mine_it_is_standing_in() {
        let app = session();
        assert_eq!(app.cursors.mine, app.state.current_mine().kind());
    }

    #[test]
    fn the_mines_cursor_walks_the_list_and_wraps_at_both_ends() {
        let mut app = browsing_mines();
        // A fresh run stands in the Stone mine, which is row zero.
        assert_eq!(app.cursors.mine, MineKind::Stone);

        app.update(Action::CursorDown);
        assert_eq!(app.cursors.mine, MineKind::Coal);
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.mine, MineKind::Stone);

        // Off the top: it comes out at the last mine. The cursor only highlights —
        // entering a mine still costs an `Enter` — so a lap of the twelve is a walk
        // and not a jump across the game.
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.mine, MineKind::Amethyst);

        // And a full lap from there, walked the whole way to prove the wrap is the
        // list's length rather than a number written down here.
        for _ in MineKind::ALL {
            app.update(Action::CursorDown);
        }
        assert_eq!(app.cursors.mine, MineKind::Amethyst);
    }

    /// A session on the Inventory tab — the second screen to answer the gestures.
    fn browsing_inventory() -> App {
        let mut app = session();
        app.screen = Screen::Inventory;
        app
    }

    #[test]
    fn the_material_cursor_walks_the_table_and_wraps_at_both_ends() {
        let mut app = browsing_inventory();
        // Nothing in the run says which material the player is looking at, so the
        // table opens at its first row.
        assert_eq!(app.cursors.material, Material::Stone);

        app.update(Action::CursorDown);
        assert_eq!(app.cursors.material, Material::Coal);
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.material, Material::Stone);

        // The same rule the Mines list keeps, and here it is kept by the same helper
        // rather than by a second copy of the arithmetic.
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.material, Material::Amethyst);

        // Walked the whole way, so the wrap is the table's length and not a number
        // written down here.
        for _ in Material::ALL {
            app.update(Action::CursorDown);
        }
        assert_eq!(app.cursors.material, Material::Amethyst);
    }

    #[test]
    fn a_list_gesture_reaches_exactly_the_cursor_of_the_open_screen() {
        // The gestures are decoded without a screen in mind, so `update` is what
        // decides who answers — and the claim is not merely that an unwired screen
        // does nothing, but that a wired one moves *its* cursor and no other. Both
        // halves are asserted for every screen, so the next one phases 6-7 wire fails
        // here rather than quietly moving two lists at once.
        for screen in Screen::ALL {
            let mut app = session();
            app.screen = screen;
            app.update(Action::CursorDown);

            let mine_moved = app.cursors.mine != MineKind::Stone;
            let material_moved = app.cursors.material != Material::Stone;

            assert_eq!(
                mine_moved,
                screen == Screen::Mines,
                "{screen:?} answered for the Mines cursor when it should not have"
            );
            assert_eq!(
                material_moved,
                screen == Screen::Inventory,
                "{screen:?} answered for the Inventory cursor when it should not have"
            );
        }
    }

    // --- The compression dialog ---

    /// A session on the Inventory tab holding one material in both denominations.
    ///
    /// The purse is stocked through the **two conversion doors and the swing**, since
    /// this crate cannot reach `Inventory::add`: `Player::inventory_mut` is
    /// `pub(crate)` in the core, deliberately, so a front-end cannot grant itself
    /// materials. Mining is the only way in, which is the same constraint phase 3 met
    /// with `Enchants::upgrade` — and it means the fixture below is a state the rules
    /// actually produce.
    fn holding_stone(app: &mut App) -> u32 {
        // A fresh run stands in the Stone mine, so a held Space is Stone in the bag.
        //
        // **Six thousand ticks, where two used to do.** The difference is the level
        // rewards: crossing a level once credited its bundle on the spot, and the
        // early bundles are half Stone, so a short session arrived at the dialog with
        // ore it had not actually mined. Rewards are collected by hand now and this
        // helper never presses the key, so the pile is swung for — which is the more
        // honest fixture besides, and what the spinner tests want two whole units of.
        for _ in 0..6_000 {
            app.state.tick(Input { space_held: true });
        }
        app.state
            .player()
            .get_inventory()
            .count(Item::Raw(Material::Stone))
    }

    fn mining_session() -> (App, u32) {
        let mut app = session();
        app.screen = Screen::Inventory;
        let raw = holding_stone(&mut app);
        (app, raw)
    }

    #[test]
    fn c_opens_the_dialog_on_the_row_under_the_cursor_at_one_unit() {
        let (mut app, _) = mining_session();

        app.update(Action::Compress);

        assert_eq!(
            app.modal,
            Some(Modal::Compress {
                material: Material::Stone,
                direction: Conversion::Compress,
                units: 1,
            }),
            "the dialog did not open at the smallest real conversion"
        );
        assert!(app.toasts.is_empty(), "an opened dialog also toasted");
    }

    /// A pile with nothing to convert gets a toast and no dialog: a modal the player
    /// could only cancel is a keypress spent on nothing.
    ///
    /// Both directions, because they fail on different denominations — a run that has
    /// mined Stone still holds no Compressed Stone, so `C` refuses where `c` succeeds.
    #[test]
    fn a_pile_with_nothing_to_convert_toasts_instead_of_opening_a_dialog() {
        // Nothing mined at all: neither direction has anything to work with.
        let mut app = session();
        app.screen = Screen::Inventory;

        app.update(Action::Compress);
        assert_eq!(app.modal, None, "an empty pile opened a dialog");
        assert!(!app.toasts.is_empty(), "an empty pile refused in silence");

        // And after mining: the raw pile converts, the Compressed one still does not.
        let raw = holding_stone(&mut app);
        assert!(raw >= RAW_PER_COMPRESSED, "the fixture mined too little");
        app.toasts = Toasts::new();

        app.update(Action::Decompress);
        assert_eq!(app.modal, None, "a run with no Compressed units opened one");
        assert!(!app.toasts.is_empty());
    }

    #[test]
    fn the_spinner_walks_and_clamps_between_one_and_the_pile() {
        let (mut app, raw) = mining_session();
        let max = raw / RAW_PER_COMPRESSED;
        assert!(max >= 2, "the fixture mined too little to walk a spinner");
        app.update(Action::Compress);

        let units = |app: &App| match app.modal {
            Some(Modal::Compress { units, .. }) => units,
            _ => 0,
        };

        app.update(Action::AdjustRight);
        assert_eq!(units(&app), 2);
        app.update(Action::AdjustLeft);
        assert_eq!(units(&app), 1);

        // The floor is one, not zero: a conversion of nothing is not something the
        // dialog should be able to offer, and `Esc` is how a player backs out.
        app.update(Action::AdjustLeft);
        assert_eq!(units(&app), 1);

        // `a` asks for everything and the clamp answers with the pile's own ceiling,
        // so the bound is read from the inventory rather than written down here.
        app.update(Action::AdjustMax);
        assert_eq!(units(&app), max);
        app.update(Action::AdjustRight);
        assert_eq!(units(&app), max, "the spinner ran past what is held");
    }

    /// The gesture the modal takes is the gesture the screen never sees — which is
    /// the whole reason `update` offers it to the modal first.
    ///
    /// Asserted on the Mines screen, where `←` has a visible effect of its own: a
    /// dialog open over it must not slide the richness dial behind the box.
    #[test]
    fn a_stacked_dialog_takes_the_gesture_before_the_screen_below_does() {
        let (mut app, _) = mining_session();
        app.update(Action::Compress);
        // The screen behind the modal is one whose `←` does something.
        app.screen = Screen::Mines;
        app.cursors.mine = MineKind::Stone;
        let setting = app.state.current_mine().get_richness_setting();

        app.update(Action::AdjustRight);

        assert_eq!(
            app.state.current_mine().get_richness_setting(),
            setting,
            "the spinner's key reached the dial behind the dialog"
        );
        assert!(matches!(app.modal, Some(Modal::Compress { units: 2, .. })));
    }

    /// `Esc` still falls through to the one `CloseModal` arm every modal shares, so
    /// the dialog closes without converting anything.
    #[test]
    fn escaping_the_dialog_converts_nothing() {
        let (mut app, raw) = mining_session();
        app.update(Action::Compress);
        app.update(Action::AdjustMax);

        app.update(Action::CloseModal);

        assert_eq!(app.modal, None);
        assert_eq!(
            app.state
                .player()
                .get_inventory()
                .count(Item::Raw(Material::Stone)),
            raw,
            "a cancelled dialog converted the pile anyway"
        );
    }

    /// The round trip through the interface: compress by hand, then decompress back.
    ///
    /// It is the conversion doors' own losslessness seen from the front-end, and it
    /// is what makes the two directions one feature — the second half is only
    /// reachable *because* the first half minted the units it spends.
    #[test]
    fn converting_by_hand_moves_the_denominations_and_announces_it() {
        let (mut app, raw) = mining_session();

        app.update(Action::Compress);
        app.update(Action::AdjustMax);
        app.update(Action::Confirm);

        let minted = raw / RAW_PER_COMPRESSED;
        let inventory = app.state.player().get_inventory();
        assert_eq!(inventory.count(Item::Compressed(Material::Stone)), minted);
        assert_eq!(
            inventory.count(Item::Raw(Material::Stone)),
            raw % RAW_PER_COMPRESSED,
            "compressing left the wrong remainder"
        );
        assert_eq!(app.modal, None, "the dialog stayed up after converting");
        assert!(!app.toasts.is_empty(), "a conversion happened in silence");

        // And back the other way, which is only possible because of the first half.
        app.update(Action::Decompress);
        app.update(Action::AdjustMax);
        app.update(Action::Confirm);

        let inventory = app.state.player().get_inventory();
        assert_eq!(inventory.count(Item::Compressed(Material::Stone)), 0);
        assert_eq!(
            inventory.count(Item::Raw(Material::Stone)),
            raw,
            "the round trip did not come back to where it started"
        );
    }

    /// The three conversion gestures belong to one screen and one modal, so every
    /// other combination is a no-op rather than a surprise.
    ///
    /// `c` from the Mine screen must not open a dialog about whatever material the
    /// Inventory cursor happens to rest on — the player is not looking at it — and
    /// `a` with nothing stacked has no value to push to its maximum.
    #[test]
    fn the_conversion_gestures_do_nothing_off_their_screen_and_off_their_modal() {
        for screen in Screen::ALL {
            if screen == Screen::Inventory {
                continue;
            }
            let mut app = session();
            app.screen = screen;
            let _ = holding_stone(&mut app);

            app.update(Action::Compress);
            app.update(Action::Decompress);
            assert_eq!(app.modal, None, "{screen:?} opened a compression dialog");
            assert!(
                app.toasts.is_empty(),
                "{screen:?} refused a key it never had"
            );
        }

        // And `a` outside the dialog: there is nothing else in the game with a
        // maximum to be pushed to, so it is deliberately inert.
        let mut app = session();
        app.screen = Screen::Inventory;
        app.update(Action::AdjustMax);
        assert_eq!(app.modal, None);
        assert!(app.toasts.is_empty());
    }

    /// The state a dialog was set against can change underneath it, and confirming
    /// then refuses rather than converting something else.
    ///
    /// Only reachable by setting the modal directly, which is the point: the spinner
    /// clamps against the *current* pile on every step, so a player cannot walk into
    /// this. What can is the tick, which credits and spends while a modal is up.
    /// The dialog closes either way — leaving it would invite the player to press
    /// `Enter` again on the same impossible number.
    #[test]
    fn confirming_a_conversion_the_pile_can_no_longer_cover_refuses_and_closes() {
        let (mut app, raw) = mining_session();
        app.modal = Some(Modal::Compress {
            material: Material::Stone,
            direction: Conversion::Compress,
            units: u32::MAX,
        });

        app.update(Action::Confirm);

        assert_eq!(app.modal, None, "the refused dialog stayed up");
        assert_eq!(
            app.state
                .player()
                .get_inventory()
                .count(Item::Raw(Material::Stone)),
            raw,
            "a refused conversion took part of the pile"
        );
        assert!(!app.toasts.is_empty(), "the refusal was not announced");
    }

    #[test]
    fn a_stacked_dialog_is_drawn_over_the_screen_it_was_opened_from() {
        let (mut app, _) = mining_session();
        app.update(Action::Compress);
        app.sync_view(Instant::now());

        let frame = whole_frame(&render_to_buffer(&app));

        assert!(frame.contains("Compress Stone"), "{frame}");
        assert!(frame.contains("Enter  do it"), "{frame}");
        // Drawn *over*, not instead of: the tab bar is still there, which is what
        // `Clear`-then-render buys and what makes an overlay cost no layout rows.
        assert!(frame.contains("3 Inventory"), "{frame}");
    }

    #[test]
    fn entering_an_open_mine_switches_the_run_and_jumps_to_the_mine_screen() {
        let mut app = browsing_mines();
        app.update(Action::CursorDown);

        app.update(Action::Confirm);

        assert_eq!(app.state.current_mine().kind(), MineKind::Coal);
        // The one screen-to-screen edge in the graph (UI-EN.md §6.1): picking a mine
        // and then having to press `1` to look at it is a chore with no decision.
        assert_eq!(app.screen, Screen::Mine);
        assert!(app.toasts.is_empty(), "an accepted mine announced itself");
    }

    #[test]
    fn a_locked_mine_is_refused_with_its_reason_and_does_not_move_the_player() {
        let mut app = browsing_mines();
        // The End mine, shut on both axes for a level-1 run with a Wooden pickaxe.
        app.cursors.mine = MineKind::Amethyst;

        app.update(Action::Confirm);

        assert_eq!(app.state.current_mine().kind(), MineKind::Stone);
        assert_eq!(app.screen, Screen::Mines, "a refusal changed the screen");
        assert_eq!(app.toasts.len(), 1, "the refusal was swallowed");
    }

    #[test]
    fn the_dial_does_not_move_past_a_ceiling_nobody_has_bought() {
        // A fresh run has bought no richness on any mine, so both arrows are no-ops
        // — and silent ones. The bar visibly stops; a toast on every repeat of a held
        // key would bury the announcements that matter.
        let mut app = browsing_mines();

        app.update(Action::AdjustRight);
        app.update(Action::AdjustLeft);

        assert_eq!(app.state.current_mine().get_richness_setting(), 0);
        assert!(app.toasts.is_empty(), "the dial's own bounds toasted");
    }

    /// The arrows address the mine under the *cursor*, and touch nothing else.
    ///
    /// A run cannot buy a richness ceiling from this crate — there is no public path
    /// to the inventory, which is why `View::sample` is a fixture at all — so what a
    /// front-end test can prove is not that the dial *moved* (the core's
    /// `the_dial_reaches_a_mine_the_player_is_not_standing_in` does that) but that
    /// the arrows are not quietly doing something else. Two things they must not do:
    /// walk the player into the mine they are pointing at, and create its grid.
    #[test]
    fn the_dial_leaves_the_run_where_it_found_it() {
        let mut app = browsing_mines();
        app.cursors.mine = MineKind::Coal;
        assert!(app.state.mine(MineKind::Coal).is_none());

        app.update(Action::AdjustRight);
        app.update(Action::AdjustLeft);

        assert_eq!(
            app.state.current_mine().kind(),
            MineKind::Stone,
            "the dial moved the player"
        );
        assert!(
            app.state.mine(MineKind::Coal).is_none(),
            "the dial built a grid for a mine nobody entered"
        );
    }

    /// What a fresh run's Mines screen actually says — the frame `cargo run` opens
    /// on, drawn from `GameState` rather than from `View::sample`.
    ///
    /// The frame tests under `screen/` render the level-23 fixture, which is a save
    /// with eleven mines visited and upgrades bought. This is the other end: a
    /// level-1 run with a Wooden pickaxe, where eleven of the twelve mines have no
    /// grid at all and most are shut. Both are worth pinning, and only this one can
    /// catch a projection that quietly needs a mine to exist before it can list it.
    #[test]
    fn a_fresh_runs_mines_screen_lists_all_twelve_and_shuts_the_right_ones() {
        let mut app = browsing_mines();
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));

        // Every mine is listed, visited or not.
        for mine in MineKind::ALL {
            assert!(
                frame.contains(mine.name()),
                "{} is missing: {frame}",
                mine.name()
            );
        }
        // The one the player is in carries the `●`, and the cursor starts on it, so
        // the `▸` wins the column — which is the rule the list applies when they
        // coincide.
        assert!(frame.contains("▸ Stone"), "{frame}");
        assert!(
            !frame.contains('●'),
            "the cursor did not win the column: {frame}"
        );

        // A level-1 run with a Wooden pickaxe: the Nether and the End are both shut,
        // and so is every mine past the Wooden gate.
        assert!(frame.contains("Lv 15  ✗"), "{frame}");
        assert!(frame.contains("Lv 30  ✗"), "{frame}");
        assert!(frame.contains("locked"), "{frame}");

        // Eleven mines have never been entered, so the pane for a mine other than
        // the standing one has no block count to give.
        app.update(Action::CursorDown);
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("never entered"), "{frame}");
    }

    // --- The Upgrades screen ---

    /// A session on the Upgrades tab, on the sub-tab named.
    pub(super) fn upgrading(tab: UpgradeTab) -> App {
        let mut app = session();
        app.screen = Screen::Upgrades;
        app.cursors.upgrade_tab = tab;
        app
    }

    /// A session over a run further along than a test can *play* to, built by writing a
    /// save and reading it back with a few fields rewritten.
    ///
    /// **A door, not a back door.** A front-end cannot mint ore — `Player::inventory_mut`
    /// is `pub(crate)` exactly so it cannot — and no enchant in the game is priced in a
    /// material the opening Stone mine drops, so *buying one* is unreachable from a run
    /// a test could mine. What a front-end can do is what it will do every launch from
    /// phase 7 on: hand [`save::from_json`] a document. That path validates before it
    /// returns, so a patch describing a run the rules could not produce is refused here
    /// rather than quietly played. The config is `()` so this crate needs no serde
    /// dependency; each patch must match, so a save-format rename fails loudly.
    fn veteran(patches: &[(&str, &str)]) -> App {
        let mut text = match save::to_json(&GameState::new(SEED, std::time::UNIX_EPOCH), &()) {
            Ok(text) => text,
            Err(error) => unreachable!("a fresh run must serialise: {error:?}"),
        };
        for (from, to) in patches {
            assert!(text.contains(from), "the save no longer contains {from:?}");
            text = text.replacen(from, to, 1);
        }
        match save::from_json::<()>(&text) {
            Ok(save) => App::new(save.state),
            Err(error) => unreachable!("a patched save must still be legal: {error:?}"),
        }
    }

    /// **The sub-tab bar and the ladder inside it are both rings.** They used to be the
    /// screen's two rules and the reason for two functions; now the pair asserts that
    /// [`UpgradeTab::next`] and [`cursor::step_index`] agree.
    #[test]
    fn the_sub_tabs_and_the_rows_inside_them_are_both_rings() {
        let mut app = upgrading(UpgradeTab::Pickaxe);

        app.update(Action::NextSubTab);
        assert_eq!(app.cursors.upgrade_tab, UpgradeTab::Enchants);
        app.update(Action::NextSubTab);
        assert_eq!(app.cursors.upgrade_tab, UpgradeTab::Mines);
        app.update(Action::NextSubTab);
        assert_eq!(
            app.cursors.upgrade_tab,
            UpgradeTab::Pickaxe,
            "the sub-tab bar did not wrap"
        );
        app.update(Action::PrevSubTab);
        assert_eq!(app.cursors.upgrade_tab, UpgradeTab::Mines, "nor backwards");

        // The ladder too. A fresh run stands on rung 0, so `↑` comes out at the last
        // rung — a maxed Netherite pickaxe, which the cursor may point at freely: it
        // highlights, and `Enter` still refuses everything past the `✓` prefix.
        app.cursors.upgrade_tab = UpgradeTab::Pickaxe;
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.pickaxe_rung, upgrade::ladder().len() - 1);
        app.update(Action::CursorDown);
        assert_eq!(app.cursors.pickaxe_rung, 0, "the last rung did not wrap");
        app.update(Action::CursorDown);
        assert_eq!(app.cursors.pickaxe_rung, 1);
    }

    /// The sub-tab binding is inert on every other screen, and the gesture is checked
    /// in the reducer as well as in the keymap — two guards for one rule, because the
    /// reducer is where a gesture's meaning is decided and a binding can be rebound.
    #[test]
    fn a_sub_tab_gesture_does_nothing_off_the_upgrades_screen() {
        let mut app = session();
        app.screen = Screen::Mines;

        app.update(Action::NextSubTab);

        assert_eq!(app.cursors.upgrade_tab, UpgradeTab::Pickaxe);
    }

    /// Each sub-tab moves its **own** cursor and leaves the other two alone — the
    /// property that makes one `↑` serve three lists.
    #[test]
    fn each_sub_tab_walks_only_its_own_list() {
        let mut app = upgrading(UpgradeTab::Enchants);
        let before = app.cursors;

        app.update(Action::CursorDown);

        assert_ne!(app.cursors.enchant, before.enchant, "the list did not move");
        assert_eq!(app.cursors.pickaxe_rung, before.pickaxe_rung);
        assert_eq!(app.cursors.mine_track, before.mine_track);

        // And the Mines sub-tab walks a mine's two rows before reaching the next mine,
        // which is what makes each row readable alone (UI.md §5.4.2).
        let mut app = upgrading(UpgradeTab::Mines);
        assert_eq!(app.cursors.mine_track, (MineKind::Stone, MineTrack::Size));
        app.update(Action::CursorDown);
        assert_eq!(
            app.cursors.mine_track,
            (MineKind::Stone, MineTrack::Richness)
        );
        app.update(Action::CursorDown);
        assert_eq!(app.cursors.mine_track, (MineKind::Coal, MineTrack::Size));
    }

    /// **The `✓` prefix, spent.** A run that has mined and compressed can buy the
    /// first rung, and the toast names the rung it *reached* rather than counting
    /// what the loop did.
    #[test]
    fn enter_buys_the_chain_up_to_the_cursor_and_names_where_it_landed() {
        let mut app = upgrading(UpgradeTab::Pickaxe);
        // Efficiency I costs 100 raw Stone, which quotes as one Compressed unit and
        // nothing loose — so the walk to the Inventory screen is mandatory even here.
        holding_stone(&mut app);
        assert_eq!(app.state.compress(Material::Stone, 1), Ok(()));
        app.cursors.pickaxe_rung = 1;

        app.update(Action::Confirm);

        let pickaxe = app.state.player().get_pickaxe();
        assert_eq!(pickaxe.enchants().get_level(EnchantType::Efficiency), 1);
        assert_eq!(app.toasts.len(), 1, "a purchase happened in silence");
    }

    /// The other branch of §8.4, and the one a fresh run hits: the value is in the
    /// bag, the *denomination* is not, so the news is "go and compress" and not "go
    /// and mine".
    #[test]
    fn a_purchase_short_of_the_denomination_asks_for_a_conversion() {
        let mut app = upgrading(UpgradeTab::Pickaxe);
        holding_stone(&mut app);
        // Deliberately *not* compressed: over a hundred raw Stone held against a price
        // of one Compressed unit.
        app.cursors.pickaxe_rung = 1;

        app.update(Action::Confirm);

        assert_eq!(
            app.state
                .player()
                .get_pickaxe()
                .enchants()
                .get_level(EnchantType::Efficiency),
            0,
            "the purchase went through on the wrong denomination"
        );
        assert_eq!(app.toasts.len(), 1, "the refusal was swallowed");
    }

    /// `M` reaches the same purchase with a further target, so a run that can afford
    /// nothing buys nothing — and the refusal is the **real** one.
    ///
    /// A penniless `M` targets the rung the player is standing on, and a chain of no
    /// purchases is affordable by definition — so without the `max(from + 1)` in
    /// [`App::buy_pickaxe_chain`] this would answer *"nothing to buy here"* to a
    /// player whose problem is that they have not mined anything.
    #[test]
    fn buy_max_on_a_penniless_run_names_the_ore_it_is_short_of() {
        let mut app = upgrading(UpgradeTab::Pickaxe);

        app.update(Action::BuyMax);

        assert_eq!(
            upgrade::position(&upgrade::ladder(), app.state.player().get_pickaxe()),
            0
        );
        assert_eq!(app.toasts.len(), 1);
        assert!(
            whole_frame(&render_to_buffer(&app)).contains("Not enough"),
            "a penniless buy-max did not name the shortage"
        );
    }

    // --- The tier-jump dip (UI.md §6.7) ---

    /// A run one rung short of Netherite, holding enough Ancient Debris to take it.
    ///
    /// The one chain in the game that *costs* power, which is the only thing that
    /// opens the modal — so every test below has to start from a pickaxe a fresh run
    /// could not have.
    fn at_the_jump() -> App {
        let mut app = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Diamond""#),
            (r#""enchants":{}"#, r#""enchants":{"Efficiency":5}"#),
            (
                r#""inventory":{}"#,
                r#""inventory":{"compressed_diamond":99,"diamond":99}"#,
            ),
        ]);
        app.screen = Screen::Upgrades;
        app.cursors.upgrade_tab = UpgradeTab::Pickaxe;
        app.cursors.pickaxe_rung =
            upgrade::position(&upgrade::ladder(), app.state.player().get_pickaxe()) + 1;
        app.sync_view(Instant::now());
        app
    }

    #[test]
    fn a_chain_that_costs_power_asks_before_it_buys() {
        // The purchase does *not* happen on `Enter`: the box opens instead, and the
        // pickaxe is still the one the player had. That ordering is the whole point of
        // §6.7 — a warning you commit to rather than one you discover.
        let mut app = at_the_jump();
        let tier = app.state.player().get_pickaxe().get_tier();

        app.update(Action::Confirm);

        assert!(matches!(app.modal, Some(Modal::Dip { buy: false, .. })));
        assert_eq!(app.state.player().get_pickaxe().get_tier(), tier);
        assert!(app.toasts.is_empty(), "a question is not news");
    }

    #[test]
    fn an_ordinary_efficiency_step_is_bought_without_a_question() {
        // The other half of the same rule: a modal on every purchase is a modal nobody
        // reads, so a climb that only gains power never opens one.
        let mut app = veteran(&[(
            r#""inventory":{}"#,
            r#""inventory":{"compressed_stone":40,"stone":99}"#,
        )]);
        app.screen = Screen::Upgrades;
        app.cursors.upgrade_tab = UpgradeTab::Pickaxe;
        app.cursors.pickaxe_rung = 1;

        app.update(Action::Confirm);

        assert!(app.modal.is_none());
        assert_eq!(
            upgrade::position(&upgrade::ladder(), app.state.player().get_pickaxe()),
            1
        );
    }

    #[test]
    fn declining_the_dip_closes_the_box_and_buys_nothing() {
        let mut app = at_the_jump();
        let tier = app.state.player().get_pickaxe().get_tier();
        app.update(Action::Confirm);

        // The caret opens on `Not yet`, so a reflex `Enter` is the *safe* answer.
        app.update(Action::Confirm);

        assert!(app.modal.is_none());
        assert_eq!(app.state.player().get_pickaxe().get_tier(), tier);
        assert!(app.toasts.is_empty(), "declining is not news");
    }

    #[test]
    fn confirming_the_dip_buys_the_chain_and_announces_it_like_any_other() {
        let mut app = at_the_jump();
        app.update(Action::Confirm);

        app.update(Action::AdjustLeft);
        app.update(Action::Confirm);

        assert!(app.modal.is_none());
        assert_eq!(
            app.state.player().get_pickaxe().get_tier(),
            PickaxeTier::Netherite
        );
        // The same sentence an undipped chain prints, because it is the same code.
        assert!(
            whole_frame(&render_to_buffer(&app)).contains("Bought Netherite Pickaxe"),
            "{}",
            whole_frame(&render_to_buffer(&app))
        );
    }

    /// **The caret clamps, it does not wrap.** Two options and a held key: a ring would
    /// put `Buy it` one repeat away from the answer the player was aiming at.
    #[test]
    fn the_dip_caret_clamps_at_both_ends() {
        let mut app = at_the_jump();
        app.update(Action::Confirm);

        app.update(Action::AdjustRight);
        assert!(matches!(app.modal, Some(Modal::Dip { buy: false, .. })));
        app.update(Action::AdjustLeft);
        app.update(Action::AdjustLeft);
        assert!(matches!(app.modal, Some(Modal::Dip { buy: true, .. })));
    }

    #[test]
    fn the_dip_box_draws_the_numbers_the_pane_behind_it_drew() {
        // One projection read twice: if these ever disagree, the player is being asked
        // to confirm a trade they were never shown.
        let mut app = at_the_jump();
        app.update(Action::Confirm);
        app.sync_view(Instant::now());

        let frame = whole_frame(&render_to_buffer(&app));
        assert!(
            frame.contains("Mining power      34.0   →   9.0"),
            "{frame}"
        );
        assert!(frame.contains("This resets Efficiency V to 0."), "{frame}");
        assert!(frame.contains("Not yet"), "{frame}");
    }

    // --- Claiming level rewards (UI.md §5.6) ---

    /// A session that has mined its way to level 3 or better, standing on the Levels
    /// screen — so there is more than one reward waiting and a lump to sweep.
    fn with_rewards_waiting() -> App {
        let mut app = session();
        app.screen = Screen::Levels;
        for _ in 0..6_000 {
            app.state.tick(Input { space_held: true });
        }
        assert!(
            app.state.player().get_level() >= 3,
            "the fixture never crossed a level"
        );
        app.sync_view(Instant::now());
        app
    }

    /// Crossing a level leaves the reward on the roadmap instead of paying it.
    ///
    /// The half of TUI phase 7's split the front-end can see: the row is marked, the
    /// footer offers the keys, and nothing has moved in the inventory.
    #[test]
    fn a_crossed_level_shows_up_on_the_roadmap_rather_than_in_the_bag() {
        let app = with_rewards_waiting();

        let waiting: Vec<u32> = app
            .view
            .levels
            .rows
            .iter()
            .filter(|row| row.unclaimed)
            .map(|row| row.level)
            .collect();
        assert!(!waiting.is_empty(), "no level was left waiting");
        assert_eq!(app.view.levels.waiting, waiting.len());
        // Level 1 is where a run starts, so it is never crossed and never waiting.
        assert!(!waiting.contains(&1));

        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("claim all"), "{frame}");
    }

    #[test]
    fn enter_claims_the_row_under_the_cursor_and_says_what_it_paid() {
        let mut app = with_rewards_waiting();
        app.cursors.level = 2;
        let before = app.state.unclaimed_count();

        app.update(Action::Confirm);

        assert_eq!(app.state.unclaimed_count(), before - 1);
        assert!(!app.state.is_unclaimed(2));
        // The toast names the level and quotes what landed, which is the same phrase
        // the roadmap's own row prints — one wording, two renderings.
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Claimed Lv 2"), "{frame}");
    }

    /// `Enter` on a row with nothing on it is announced, unlike the richness dial's
    /// silent clamp: the mark column says whether there is anything there, so this is
    /// a press against what the screen already told the player.
    #[test]
    fn enter_on_a_row_with_nothing_waiting_says_so() {
        let mut app = with_rewards_waiting();
        // A level far past the player's — reached by nobody, so waiting for nobody.
        app.cursors.level = LEVEL_CAP;
        let before = app.state.unclaimed_count();

        app.update(Action::Confirm);

        assert_eq!(app.state.unclaimed_count(), before, "a claim leaked");
        assert!(!app.toasts.is_empty(), "an empty row refused in silence");
    }

    #[test]
    fn claim_all_empties_the_ladder_in_one_announcement() {
        let mut app = with_rewards_waiting();
        let waiting = app.state.unclaimed_count();
        assert!(waiting >= 2, "a sweep of one level tests nothing");

        app.update(Action::ClaimAll);

        assert_eq!(app.state.unclaimed_count(), 0);
        // **One toast for the lump.** Six three-second toasts stacked on each other
        // are six the player reads none of, which is the same argument the refusal
        // wording makes for naming only the first shortfall.
        assert_eq!(app.toasts.len(), 1, "the sweep announced level by level");
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(
            frame.contains(&format!("Claimed {waiting} levels")),
            "{frame}"
        );
        // And the footer stops offering keys that would now refuse.
        assert!(!frame.contains("claim all"), "{frame}");
    }

    /// The sweep's three shapes of sentence, since each is a different arm.
    ///
    /// A sweep of exactly one level names it, the way `Enter` would — announcing
    /// `Claimed 1 levels` for a single bundle would be a plural and a lost number at
    /// the same time. A sweep of nothing says so and is neutral: the key is only
    /// advertised when something is waiting, so an empty press is not a mistake about
    /// any particular level.
    #[test]
    fn a_sweep_of_one_names_it_and_a_sweep_of_none_says_so() {
        let mut app = with_rewards_waiting();
        // Down to a single reward, collected by hand through the other door.
        while app.state.unclaimed_count() > 1 {
            let level = (1..=LEVEL_CAP).find(|&level| app.state.is_unclaimed(level));
            match level {
                Some(level) => assert!(app.state.claim_level(level).is_ok()),
                None => unreachable!("the count says one is left"),
            }
        }

        app.update(Action::ClaimAll);
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Claimed Lv"), "{frame}");
        assert!(
            !frame.contains("levels"),
            "a single bundle was pluralised: {frame}"
        );

        // And again, against a ladder with nothing on it.
        app.toasts = Toasts::new();
        app.update(Action::ClaimAll);
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Nothing waiting"), "{frame}");
    }

    #[test]
    fn the_claim_gestures_do_nothing_off_the_levels_screen() {
        for screen in [
            Screen::Mine,
            Screen::Mines,
            Screen::Inventory,
            Screen::Upgrades,
        ] {
            let mut app = with_rewards_waiting();
            app.screen = screen;
            let before = app.state.unclaimed_count();

            app.update(Action::ClaimAll);
            app.update(Action::Confirm);

            assert_eq!(
                app.state.unclaimed_count(),
                before,
                "{screen:?} claimed a reward it does not own"
            );
        }
    }

    /// The cursor walks the ladder, wraps at both ends, and `Home` brings it back.
    ///
    /// The longest list in the game, and the one where the wrap earns the most: `Home`
    /// is the *other* way back to where the player stands, and the two must not fight —
    /// so this asserts a lap and a jump in the same run.
    #[test]
    fn the_roadmap_cursor_wraps_at_both_ends_and_home_returns_to_the_player() {
        let mut app = with_rewards_waiting();
        let here = app.state.player().get_level();
        assert_eq!(app.cursors.level, 1, "the session opened above level 1");

        app.update(Action::CursorUp);
        assert_eq!(
            app.cursors.level, LEVEL_CAP,
            "the first rung did not wrap to the cap"
        );
        app.update(Action::CursorDown);
        assert_eq!(app.cursors.level, 1, "nor the cap back to the first rung");

        // A full lap, so the wrap is the ladder's own length: `LEVEL_CAP` steps from
        // rung one land back on rung one, and the level stays one-based throughout.
        for _ in 0..LEVEL_CAP {
            app.update(Action::CursorDown);
        }
        assert_eq!(app.cursors.level, 1, "a full lap did not come back");

        app.update(Action::JumpToCurrent);
        assert_eq!(app.cursors.level, here);
    }

    /// The history is a **list**, so `↑↓` wraps it — `docs/UI.md` §9's test being
    /// *list or quantity*, and a log of sentences is not a quantity.
    ///
    /// The cursor is a rank counted from the newest, so `0` is the top of the panel and
    /// stepping *down* walks backwards in time.
    #[test]
    fn the_history_cursor_wraps_at_both_ends() {
        let mut app = session();
        app.screen = Screen::Stats;
        for index in 0..4 {
            app.toasts
                .push(format!("entry {index}"), Tone::Neutral, TOAST_TTL);
        }

        assert_eq!(app.cursors.history, 0, "the session opened part-way down");

        app.update(Action::CursorUp);
        assert_eq!(
            app.cursors.history, 3,
            "the newest entry did not wrap to the oldest"
        );
        app.update(Action::CursorDown);
        assert_eq!(app.cursors.history, 0, "nor the oldest back to the newest");

        // A full lap is the log's own length.
        for _ in 0..4 {
            app.update(Action::CursorDown);
        }
        assert_eq!(app.cursors.history, 0, "a full lap did not come back");
    }

    /// A log with nothing in it has no row to land on, and a `↑` on it must not be the
    /// keypress that takes the process down: `step_index` answers `0` for an empty
    /// list rather than dividing by zero.
    #[test]
    fn scrolling_a_history_that_has_announced_nothing_is_harmless() {
        let mut app = session();
        app.screen = Screen::Stats;
        assert!(app.toasts.is_empty(), "the fixture must start silent");

        app.update(Action::CursorUp);
        app.update(Action::CursorDown);

        assert_eq!(app.cursors.history, 0);
    }

    /// The gesture belongs to the screen that owns the list. `↑↓` on Stats must not
    /// move the Mines or Levels cursor, and `↑↓` elsewhere must not move this one —
    /// which is the whole reason [`App::step_list_cursor`] chooses on the screen
    /// rather than the keymap doing it.
    #[test]
    fn the_history_cursor_only_moves_on_the_screen_that_owns_it() {
        let mut app = session();
        for index in 0..4 {
            app.toasts
                .push(format!("entry {index}"), Tone::Neutral, TOAST_TTL);
        }

        for screen in Screen::ALL {
            app.screen = screen;
            app.cursors.history = 0;
            app.update(Action::CursorDown);

            let moved = app.cursors.history != 0;
            assert_eq!(
                moved,
                screen == Screen::Stats,
                "{screen:?} answered for a list it does not own"
            );
        }
    }

    // --- The compress-first loop (UI.md §8.4) ---

    /// A run holding the *value* of a pickaxe rung but not its *denomination*.
    ///
    /// Wooden Efficiency I is priced at 100 Stone, which quotes as `1 Compressed + 0`
    /// — so a player sitting on 100 raw Stone is exactly rich enough and is still
    /// refused. That is the whole of §8.4 in one purse.
    fn holding_the_value_but_not_the_denomination() -> App {
        let mut app = veteran(&[(r#""inventory":{}"#, r#""inventory":{"stone":150}"#)]);
        app.screen = Screen::Upgrades;
        app.cursors.upgrade_tab = UpgradeTab::Pickaxe;
        app.cursors.pickaxe_rung = 1;
        app
    }

    #[test]
    fn a_refusal_of_the_denomination_is_remembered_for_the_inventory_screen() {
        let mut app = holding_the_value_but_not_the_denomination();

        app.update(Action::Confirm);

        // The toast routes the player; the memory is what greets them when they get
        // there. Both are needed — the toast is gone in a few seconds.
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Compress first"), "{frame}");
        match &app.refused {
            Some(hint) => {
                assert_eq!(hint.purchase, "Wooden Eff I");
                assert_eq!(hint.needed.material, Material::Stone);
                assert_eq!(hint.needed.compressed, 1);
            }
            None => unreachable!("a compress-first refusal must be remembered"),
        }
    }

    #[test]
    fn the_inventory_panel_names_the_refusal_on_its_own_row_and_no_other() {
        let mut app = holding_the_value_but_not_the_denomination();
        app.update(Action::Confirm);
        app.screen = Screen::Inventory;

        // Stone is `Material::ALL`'s first row, so a fresh Inventory cursor is already
        // on the pile the refusal is about.
        app.cursors.material = Material::Stone;
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Wooden Eff I wants"), "{frame}");

        // Move to any other pile and the note goes with it: a price in Stone printed
        // beside the Coal row would attach a number to the wrong thing.
        app.cursors.material = Material::Coal;
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(!frame.contains("wants"), "{frame}");
    }

    #[test]
    fn compressing_by_hand_and_coming_back_completes_the_loop() {
        // The §8.4 walk, end to end and **entirely through gestures**: refused, `c` to
        // walk to the pile that was named, `c` and `Enter` to convert, back to Upgrades
        // — and the cursor is still on the rung they left, because it lives in
        // `Cursors` and never went anywhere.
        //
        // The two legs the player used to walk by hand are the two `Action`s at the
        // top: this test set `screen` and `cursors.material` itself before `GoCompress`
        // existed, which is precisely the work the key took over.
        let mut app = holding_the_value_but_not_the_denomination();
        app.update(Action::Confirm);
        assert!(app.refused.is_some());

        app.update(Action::GoCompress);
        assert_eq!(app.screen, Screen::Inventory);
        assert_eq!(app.cursors.material, Material::Stone);
        app.update(Action::Compress);
        app.update(Action::Confirm);

        app.screen = Screen::Upgrades;
        assert_eq!(app.cursors.pickaxe_rung, 1, "the selection was not kept");
        app.update(Action::Confirm);

        assert_eq!(
            upgrade::position(&upgrade::ladder(), app.state.player().get_pickaxe()),
            1
        );
        assert!(
            app.refused.is_none(),
            "a cleared refusal was still remembered"
        );
    }

    /// The walk lands on the pile the refusal names, not on the one the cursor
    /// happened to be resting on — which is the only reason the key beats pressing `3`.
    #[test]
    fn the_walk_lands_on_the_pile_the_refusal_named() {
        // Fortune is priced in Emerald, and the Inventory cursor opens on Stone: two
        // different rows, so a cursor that failed to move would still look right on a
        // Stone-priced refusal and be caught only here.
        let mut app = veteran(&[(r#""inventory":{}"#, r#""inventory":{"emerald":1500}"#)]);
        app.screen = Screen::Upgrades;
        app.cursors.upgrade_tab = UpgradeTab::Enchants;
        app.cursors.enchant = EnchantType::Fortune;
        assert_eq!(app.cursors.material, Material::Stone);

        app.update(Action::Confirm);
        app.update(Action::GoCompress);

        assert_eq!(app.screen, Screen::Inventory);
        assert_eq!(app.cursors.material, Material::Emerald);
        // The refusal survives the walk: it is what the screen just reached prints the
        // hint from, so consuming it here would empty the panel this key exists to
        // reach.
        assert!(
            app.refused.is_some(),
            "the walk ate the note it was carrying"
        );
        // `render` draws the cached projection, and the walk moved a cursor rather
        // than the run — so the reprojection `run` does before each draw has to be
        // asked for by hand here.
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Fortune I wants"), "{frame}");
    }

    /// A `c` with nothing refused still travels, and moves no cursor.
    ///
    /// The alternative was a key that does nothing at all. It is only ever advertised
    /// by a refusal, so this is the rare press — and landing on the Inventory is the
    /// obvious half of what the key means even with no pile to point at.
    #[test]
    fn the_walk_with_nothing_refused_travels_and_moves_no_cursor() {
        let mut app = upgrading(UpgradeTab::Pickaxe);
        app.cursors.material = Material::Diamond;
        assert!(app.refused.is_none());

        app.update(Action::GoCompress);

        assert_eq!(app.screen, Screen::Inventory);
        assert_eq!(app.cursors.material, Material::Diamond);
    }

    #[test]
    fn a_shortage_the_inventory_cannot_fix_is_not_remembered() {
        // `Insufficient` is answered by mining. Leaving a note on the Inventory screen
        // would send the player somewhere that can do nothing for them.
        let mut app = upgrading(UpgradeTab::Pickaxe);
        app.cursors.pickaxe_rung = 1;

        app.update(Action::Confirm);

        assert!(app.refused.is_none());
        assert!(
            whole_frame(&render_to_buffer(&app)).contains("Not enough"),
            "{}",
            whole_frame(&render_to_buffer(&app))
        );
    }

    #[test]
    fn a_refused_enchant_is_remembered_by_the_level_it_was_refused_at() {
        // 10 Compressed Emerald for Fortune I, against a purse holding the value in
        // raw: the enchant track's own instance of the same refusal.
        let mut app = veteran(&[(r#""inventory":{}"#, r#""inventory":{"emerald":1500}"#)]);
        app.screen = Screen::Upgrades;
        app.cursors.upgrade_tab = UpgradeTab::Enchants;
        app.cursors.enchant = EnchantType::Fortune;

        app.update(Action::Confirm);

        match &app.refused {
            Some(hint) => {
                assert_eq!(hint.purchase, "Fortune I");
                assert_eq!(hint.needed.material, Material::Emerald);
            }
            None => unreachable!("a compress-first refusal must be remembered"),
        }
    }

    #[test]
    fn a_refused_mine_track_is_remembered_by_the_level_it_was_refused_at() {
        // Both tracks, because each is priced by its own curve and each names itself
        // in the hint — a player who was refused a richness ceiling should not be sent
        // to the Inventory to buy a size.
        for (track, purchase) in [
            (MineTrack::Size, "Stone size 2"),
            (MineTrack::Richness, "Stone richness 2"),
        ] {
            let mut app = veteran(&[(r#""inventory":{}"#, r#""inventory":{"stone":150}"#)]);
            app.screen = Screen::Upgrades;
            app.cursors.upgrade_tab = UpgradeTab::Mines;
            app.cursors.mine_track = (MineKind::Stone, track);

            app.update(Action::Confirm);

            match &app.refused {
                Some(hint) => {
                    assert_eq!(hint.purchase, purchase);
                    assert_eq!(hint.needed.material, Material::Stone);
                }
                None => unreachable!("a compress-first refusal must be remembered"),
            }
        }
    }

    /// The top of the ladder: `Enter` there buys nothing, and there is no rung past it
    /// whose price could be quoted — so nothing is remembered rather than a blank one.
    #[test]
    fn a_chain_that_stops_at_the_end_of_the_ladder_remembers_nothing() {
        let mut app = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Netherite""#),
            (r#""enchants":{}"#, r#""enchants":{"Efficiency":15}"#),
        ]);
        app.screen = Screen::Upgrades;
        app.cursors.upgrade_tab = UpgradeTab::Pickaxe;
        app.cursors.pickaxe_rung = upgrade::ladder().len() - 1;

        app.update(Action::Confirm);

        assert!(app.refused.is_none());
        assert!(
            whole_frame(&render_to_buffer(&app)).contains("Nothing to buy here"),
            "{}",
            whole_frame(&render_to_buffer(&app))
        );
    }

    /// **`n` and `Esc` close it through the path every modal shares.** `update_modal`
    /// *declines* [`Action::CloseModal`] — that is what its catch-all is for — and the
    /// reducer's one `CloseModal` arm does the closing, so there is a single
    /// implementation of *shut the box* rather than one per modal.
    #[test]
    fn declining_the_dip_goes_through_the_close_every_modal_shares() {
        let mut app = at_the_jump();
        app.update(Action::Confirm);
        let tier = app.state.player().get_pickaxe().get_tier();

        app.update(Action::CloseModal);

        assert!(app.modal.is_none());
        assert_eq!(app.state.player().get_pickaxe().get_tier(), tier);
    }

    #[test]
    fn a_bought_enchant_level_is_announced_by_its_new_roman_numeral() {
        // Fortune I is priced at 10 Compressed Emerald — a material the Overworld's
        // opening mine does not drop, which is why this needs a run that has been
        // somewhere. The toast names the level *reached*, not the one paid for: the
        // player asked for "one more", and the answer they want is where they are now.
        let mut app = veteran(&[(
            r#""inventory":{}"#,
            r#""inventory":{"compressed_emerald":10}"#,
        )]);
        app.screen = Screen::Upgrades;
        app.cursors.upgrade_tab = UpgradeTab::Enchants;
        app.cursors.enchant = EnchantType::Fortune;

        app.update(Action::Confirm);

        assert_eq!(
            app.state
                .player()
                .get_pickaxe()
                .enchants()
                .get_level(EnchantType::Fortune),
            1
        );
        assert!(
            whole_frame(&render_to_buffer(&app)).contains("Bought Fortune I"),
            "{}",
            whole_frame(&render_to_buffer(&app))
        );
    }

    /// The cursor at or behind the rung the player stands on: there is genuinely
    /// nothing to buy, and *that* is what the toast should say. The one path to the
    /// [`Affordability::Affordable`] arm of the announcement.
    #[test]
    fn enter_on_a_rung_already_owned_says_there_is_nothing_to_buy() {
        let mut app = upgrading(UpgradeTab::Pickaxe);
        app.cursors.pickaxe_rung = 0;

        app.update(Action::Confirm);

        assert_eq!(app.toasts.len(), 1);
        assert!(
            whole_frame(&render_to_buffer(&app)).contains("Nothing to buy"),
            "the standing rung reported a shortage instead"
        );
    }

    /// The defect this wording was rewritten for: the pane prices `Wooden Eff I` at
    /// `1 Compressed Stone` and the toast under it used to answer `100 Stone`.
    ///
    /// Two numbers for one price, side by side, and the player has no way to know they
    /// are the same one. The shortfall is still the core's — raw, because that pass has
    /// no denomination to give — and only the *wording* re-splits it.
    #[test]
    fn a_price_is_refused_in_the_denomination_the_pane_quotes_it_in() {
        let mut app = upgrading(UpgradeTab::Pickaxe);
        app.cursors.pickaxe_rung = 1;

        app.update(Action::Confirm);

        let frame = whole_frame(&render_to_buffer(&app));
        assert!(
            frame.contains("Not enough Stone — 1 Compressed needed, 0 held"),
            "the toast still speaks raw: {frame}"
        );
    }

    /// The same sentence from the other door, which used to print `CoreError`'s own.
    ///
    /// The enchant and mine tracks fell through to
    /// [`App::announce_core_refusal`] — a different shape, and one that skips
    /// [`grouped`], so `10 Compressed Emerald` was the only kind of price in the game
    /// whose thousands went unseparated.
    #[test]
    fn every_purchase_door_refuses_in_the_same_sentence() {
        let mut app = upgrading(UpgradeTab::Enchants);

        app.update(Action::Confirm);

        let frame = whole_frame(&render_to_buffer(&app));
        assert!(
            frame.contains("Not enough Emerald — 10 Compressed needed, 0 held"),
            "the enchant door kept the core's wording: {frame}"
        );
    }

    /// **The two totality guards in the announcement**, reached directly because
    /// nothing in the core produces them: a refusal always carries at least one
    /// shortfall, so an empty list is the "cannot happen" this crate answers rather
    /// than traps on.
    #[test]
    fn a_refusal_carrying_no_shortfall_still_says_which_loop_to_run() {
        // One session each: only the **newest** toast is drawn, deliberately, so two
        // pushed into one session would leave the first unassertable.
        for (verdict, expected) in [
            (Affordability::CompressFirst(Vec::new()), "Compress first"),
            (Affordability::Insufficient(Vec::new()), "Not enough ore"),
        ] {
            let mut app = session();
            app.announce_refusal(&verdict);

            let frame = whole_frame(&render_to_buffer(&app));
            assert!(frame.contains(expected), "{verdict:?} drew {frame}");
        }
    }

    /// `Enter` means nothing on a screen that sells nothing and selects nothing —
    /// the catch-all arm, which must stay a no-op rather than falling through to
    /// whichever screen was wired last.
    #[test]
    fn confirm_does_nothing_on_a_screen_that_owns_no_selection() {
        let mut app = session();
        app.screen = Screen::Stats;
        let before = app.cursors;

        app.update(Action::Confirm);

        assert_eq!(app.cursors, before);
        assert!(app.toasts.is_empty());
    }

    /// **A mine track bought for real**, which is as far as a front-end test can get:
    /// the size track of the opening mine is priced in the one ore a fresh run can
    /// actually dig.
    #[test]
    fn a_mine_track_is_bought_for_the_mine_under_the_cursor() {
        let mut app = upgrading(UpgradeTab::Mines);
        holding_stone(&mut app);
        assert_eq!(app.state.compress(Material::Stone, 1), Ok(()));
        app.cursors.mine_track = (MineKind::Stone, MineTrack::Size);
        let before = app.state.current_mine().get_size_level();

        app.update(Action::Confirm);

        assert_eq!(app.state.current_mine().get_size_level(), before + 1);
        assert!(
            whole_frame(&render_to_buffer(&app)).contains("Stone size"),
            "the purchase was not announced by name"
        );
    }

    /// `M` on the same track spends everything the purse allows in one keypress, and
    /// the richness *ceiling* is the other of the two tracks — the one whose name has
    /// to stay apart from the free dial on the Mines screen.
    #[test]
    fn buy_max_on_a_mine_track_spends_what_it_can_and_stops() {
        let mut app = upgrading(UpgradeTab::Mines);
        holding_stone(&mut app);
        assert_eq!(app.state.compress(Material::Stone, 1), Ok(()));
        app.cursors.mine_track = (MineKind::Stone, MineTrack::Richness);

        app.update(Action::BuyMax);

        assert!(
            app.state.current_mine().get_richness_level() >= 1,
            "buy-max bought nothing it could afford"
        );
        assert!(
            whole_frame(&render_to_buffer(&app)).contains("Stone richness"),
            "the purchase was not announced by name"
        );
    }

    /// `M` on the enchant track is the same gesture again — and on a fresh run it
    /// stops immediately, since no enchant in the game is priced in an ore the
    /// opening mine drops.
    #[test]
    fn buy_max_on_an_unaffordable_enchant_track_stops_at_once() {
        let mut app = upgrading(UpgradeTab::Enchants);

        app.update(Action::BuyMax);

        assert_eq!(
            app.state
                .player()
                .get_pickaxe()
                .enchants()
                .get_level(app.cursors.enchant),
            0
        );
        assert_eq!(app.toasts.len(), 1);
    }

    /// **A mine the run has never opened refuses, and says where to go.** Enoal's
    /// call for phase 6: the grid is minted lazily, and minting one to upgrade it
    /// would let a purchase advance the run's dice.
    #[test]
    fn upgrading_an_unvisited_mine_sends_the_player_there_instead() {
        // Both tracks, because they are two doors onto one rule and a guard on only
        // one of them is exactly the shape this would fail as.
        for track in MineTrack::ALL {
            let mut app = upgrading(UpgradeTab::Mines);
            app.cursors.mine_track = (MineKind::Coal, track);

            app.update(Action::Confirm);

            assert!(
                app.state.mine(MineKind::Coal).is_none(),
                "a grid was minted"
            );
            assert_eq!(app.toasts.len(), 1, "{track:?} was refused in silence");
            let frame = whole_frame(&render_to_buffer(&app));
            assert!(
                frame.contains("enter the Coal mine"),
                "the refusal did not say where to go"
            );
            // **The arm that keeps the unified refusal honest.** Every priced door now
            // words its shortfall itself, and this one has no price: an unopened mine
            // has no level to quote from. Re-phrasing it would invent a shortage the
            // player does not have and send them to a mine face instead of to `2 Mines`.
            assert!(
                !frame.contains("Not enough"),
                "an unopened mine reported a shortage instead"
            );
        }
    }

    /// The enchant track refuses in the core's own words, verbatim — a maxed track, a
    /// missing ore and a shut world each already say what they are.
    #[test]
    fn an_unaffordable_enchant_is_refused_in_the_cores_own_words() {
        let mut app = upgrading(UpgradeTab::Enchants);
        let before = app
            .state
            .player()
            .get_pickaxe()
            .enchants()
            .get_level(app.cursors.enchant);

        app.update(Action::Confirm);

        assert_eq!(
            app.state
                .player()
                .get_pickaxe()
                .enchants()
                .get_level(app.cursors.enchant),
            before
        );
        assert_eq!(app.toasts.len(), 1);
    }

    // --- The prestige flow (UI.md §6.8, §6.9) ---

    /// A run standing at both gates with the price in hand.
    ///
    /// Level 50 and Netherite is the whole of `prestige::lock`, and rank 0's price is
    /// `61 Compressed` of Amethyst — so the purse is quoted in that denomination and
    /// not in raw, which would be the *other* refusal. Reached through
    /// [`veteran`](self) because no test can play to the level cap.
    fn ready_to_prestige() -> App {
        let mut app = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Netherite""#),
            (r#""level":1,"#, r#""level":50,"#),
            (
                r#""inventory":{}"#,
                r#""inventory":{"compressed_amethyst":65}"#,
            ),
        ]);
        app.screen = Screen::Stats;
        app
    }

    /// The same run with the ore in the wrong denomination — rich enough, still refused.
    fn holding_raw_amethyst() -> App {
        let mut app = veteran(&[
            (r#""tier":"Wooden""#, r#""tier":"Netherite""#),
            (r#""level":1,"#, r#""level":50,"#),
            (r#""inventory":{}"#, r#""inventory":{"amethyst":9999}"#),
        ]);
        app.screen = Screen::Stats;
        app
    }

    #[test]
    fn p_opens_the_preview_on_stats_and_nowhere_else() {
        let mut app = session();
        app.screen = Screen::Stats;
        assert_eq!(
            keymap::resolve(&app, KeyEvent::from(KeyCode::Char('p'))),
            Some(Action::OpenPrestige)
        );
        app.update(Action::OpenPrestige);
        assert_eq!(app.modal, Some(Modal::PrestigePreview));

        // The gesture is guarded in the reducer too, so a future binding elsewhere
        // cannot open the box from a screen that does not lead there.
        for screen in Screen::ALL {
            if screen == Screen::Stats {
                continue;
            }
            let mut app = session();
            app.screen = screen;
            app.update(Action::OpenPrestige);
            assert_eq!(app.modal, None, "{screen:?} opened the prestige preview");
        }
    }

    /// The preview is readable long before it is usable — that is what makes the End's
    /// richness dial a decision rather than a curiosity (§6.8).
    #[test]
    fn a_locked_run_may_read_the_preview_and_is_refused_at_the_gate() {
        let mut app = session();
        app.screen = Screen::Stats;
        app.update(Action::OpenPrestige);
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("You lose"), "{frame}");

        app.update(Action::Confirm);

        // The box stays up — it is what explains the refusal — and the core's own
        // sentence is raised over it.
        assert_eq!(app.modal, Some(Modal::PrestigePreview));
        assert_eq!(app.toasts.len(), 1);
        assert_eq!(app.state.player().get_prestige(), 0);
    }

    /// Rich in value, wrong in shape: the refusal that sends a player to `3 Inventory`
    /// rather than back to a mine.
    #[test]
    fn the_wrong_denomination_is_refused_without_opening_the_confirm() {
        let mut app = holding_raw_amethyst();
        app.update(Action::OpenPrestige);
        app.update(Action::Confirm);

        assert_eq!(app.modal, Some(Modal::PrestigePreview));
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Compress first"), "{frame}");
    }

    /// **The prestige is the §8.4 loop's fourth door**, and it has to be: its price is
    /// quoted in two denominations like every other, so a player holding the value in
    /// raw is refused here exactly as they are on the Upgrades screen. The walk is
    /// claimed inside the modal, because the modal is what would otherwise swallow `c`.
    #[test]
    fn c_walks_from_the_refused_preview_to_the_pile_it_named() {
        let mut app = holding_raw_amethyst();
        app.update(Action::OpenPrestige);
        app.update(Action::Confirm);

        // The refusal is remembered, so the Inventory screen has something to say.
        match &app.refused {
            Some(hint) => assert_eq!(hint.needed.material, Material::Amethyst),
            None => unreachable!("a compress-first prestige must be remembered"),
        }
        // And the box advertises the key, since a modal leaves no footer to read.
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("press  c  to go"), "{frame}");

        assert_eq!(
            keymap::resolve(&app, KeyEvent::from(KeyCode::Char('c'))),
            Some(Action::GoCompress)
        );
        app.update(Action::GoCompress);

        // The box closes on the way out, or it would capture the keys the player went
        // to the Inventory to press.
        assert_eq!(app.modal, None);
        assert_eq!(app.screen, Screen::Inventory);
        assert_eq!(app.cursors.material, Material::Amethyst);
        app.sync_view(Instant::now());
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Prestige I"), "{frame}");
    }

    /// The memory is cleared the moment it stops being true, like every other refusal.
    #[test]
    fn opening_the_confirm_forgets_a_refusal_the_run_has_outgrown() {
        let mut app = ready_to_prestige();
        app.refused = Some(CompressHint {
            purchase: "Wooden Eff I".to_owned(),
            needed: economy::CostLine::from_raw_total(Material::Stone, 100),
        });

        app.update(Action::OpenPrestige);
        app.update(Action::Confirm);

        assert!(app.refused.is_none());
    }

    #[test]
    fn an_affordable_preview_opens_the_confirm_on_an_empty_field() {
        let mut app = ready_to_prestige();
        app.update(Action::OpenPrestige);
        app.update(Action::Confirm);

        assert_eq!(
            app.modal,
            Some(Modal::PrestigeConfirm {
                typed: String::new()
            })
        );
        // Nothing has been spent by opening the box.
        assert_eq!(app.state.player().get_prestige(), 0);
    }

    /// The field echoes what was typed and stops at the word's own length — a player
    /// meets their mistake rather than typing past it.
    #[test]
    fn the_field_takes_letters_erases_them_and_stops_at_the_words_length() {
        let mut app = ready_to_prestige();
        app.modal = Some(Modal::PrestigeConfirm {
            typed: String::new(),
        });
        for character in "prez".chars() {
            app.update(Action::TypeChar(character));
        }
        assert_eq!(
            app.modal,
            Some(Modal::PrestigeConfirm {
                typed: "prez".to_owned()
            })
        );

        app.update(Action::EraseChar);
        assert_eq!(
            app.modal,
            Some(Modal::PrestigeConfirm {
                typed: "pre".to_owned()
            })
        );

        for _ in 0..40 {
            app.update(Action::TypeChar('X'));
        }
        match &app.modal {
            Some(Modal::PrestigeConfirm { typed }) => {
                assert_eq!(typed.chars().count(), CONFIRM_WORD.chars().count());
            }
            other => unreachable!("the confirm closed: {other:?}"),
        }

        // Erasing an empty field is a no-op rather than an underflow.
        app.modal = Some(Modal::PrestigeConfirm {
            typed: String::new(),
        });
        app.update(Action::EraseChar);
        assert_eq!(
            app.modal,
            Some(Modal::PrestigeConfirm {
                typed: String::new()
            })
        );
    }

    /// The whole point of §6.9: the wrong word does not buy the right thing.
    #[test]
    fn a_wrong_word_neither_prestiges_nor_says_anything() {
        let mut app = ready_to_prestige();
        app.modal = Some(Modal::PrestigeConfirm {
            typed: "prestige".to_owned(),
        });
        app.update(Action::Confirm);

        assert_eq!(app.state.player().get_prestige(), 0);
        assert_eq!(app.toasts.len(), 0);
        // The box stays up with the word still in it, which is the answer.
        assert_eq!(
            app.modal,
            Some(Modal::PrestigeConfirm {
                typed: "prestige".to_owned()
            })
        );
    }

    #[test]
    fn the_typed_word_trades_the_run_in_and_resets_the_front_end_with_it() {
        let mut app = ready_to_prestige();
        app.cursors.pickaxe_rung = 20;
        app.cursors.level = 50;
        app.update(Action::OpenPrestige);
        app.update(Action::Confirm);
        for character in CONFIRM_WORD.chars() {
            app.update(Action::TypeChar(character));
        }
        app.update(Action::Confirm);

        let player = app.state.player();
        assert_eq!(player.get_prestige(), 1);
        assert_eq!(player.get_level(), 1);
        assert_eq!(player.get_pickaxe().get_tier(), PickaxeTier::Wooden);

        // The front-end's own state follows the run, or a cursor points at a rung the
        // player no longer stands on.
        assert_eq!(app.modal, None);
        assert_eq!(app.screen, Screen::Mine);
        assert_eq!(app.cursors.mine, MineKind::Stone);
        assert_eq!(app.cursors.pickaxe_rung, 0);
        assert_eq!(app.cursors.level, 1);
        assert!(app.refused.is_none());

        let frame = {
            app.sync_view(Instant::now());
            whole_frame(&render_to_buffer(&app))
        };
        assert!(frame.contains("Prestige I — ×1.10"), "{frame}");
    }

    /// `Esc` at either step leaves the run exactly where it was.
    #[test]
    fn escaping_either_box_trades_nothing() {
        for typed in [None, Some(String::new())] {
            let mut app = ready_to_prestige();
            app.modal = match typed {
                Some(typed) => Some(Modal::PrestigeConfirm { typed }),
                None => Some(Modal::PrestigePreview),
            };
            app.update(Action::CloseModal);
            assert_eq!(app.modal, None);
            assert_eq!(app.state.player().get_prestige(), 0);
            assert_eq!(app.state.player().get_level(), 50);
        }
    }

    /// The confirm claims every letter, which is what makes typing eight of them an
    /// affordance muscle memory cannot produce by accident.
    #[test]
    fn the_confirm_captures_the_letters_the_ring_would_otherwise_claim() {
        let mut app = ready_to_prestige();
        app.modal = Some(Modal::PrestigeConfirm {
            typed: String::new(),
        });
        assert_eq!(
            keymap::resolve(&app, KeyEvent::from(KeyCode::Char('q'))),
            Some(Action::TypeChar('q'))
        );
        assert_eq!(
            keymap::resolve(&app, KeyEvent::from(KeyCode::Char('1'))),
            Some(Action::TypeChar('1'))
        );
        assert_eq!(keymap::resolve(&app, KeyEvent::from(KeyCode::Tab)), None);
        // Ctrl-C outranks the capture, as it does for every modal.
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(keymap::resolve(&app, ctrl_c), Some(Action::Quit));
    }

    /// Text is the confirm's alone. A `TypeChar` reaching the reducer with no box open
    /// is not a state the keymap can produce — it decodes letters only under that
    /// modal — and the arm exists so it is a no-op rather than an omission.
    #[test]
    fn a_typed_character_outside_the_confirm_changes_nothing() {
        for screen in Screen::ALL {
            let mut app = session();
            app.screen = screen;
            app.update(Action::TypeChar('P'));
            app.update(Action::EraseChar);
            assert_eq!(app.modal, None);
            assert_eq!(app.screen, screen);
            assert_eq!(app.toasts.len(), 0);
        }
    }

    /// **The till is the authority, not the projection.** The confirm is only reachable
    /// through an affordable preview, so this state is unreachable in play — which is
    /// exactly why the outcome is routed rather than assumed: the box is open against a
    /// run the rules refuse, and the refusal is the core's own sentence.
    #[test]
    fn a_confirm_the_run_cannot_pay_refuses_at_the_till() {
        let mut app = session();
        app.screen = Screen::Stats;
        app.modal = Some(Modal::PrestigeConfirm {
            typed: CONFIRM_WORD.to_owned(),
        });

        app.update(Action::Confirm);

        assert_eq!(app.state.player().get_prestige(), 0);
        assert_eq!(app.modal, None);
        assert_eq!(app.toasts.len(), 1);
        // Nothing moved: a refusal that changes the screen would be a refusal that
        // half-happened.
        assert_eq!(app.screen, Screen::Stats);
    }

    #[test]
    fn the_confirm_is_drawn_over_the_screen_it_was_opened_from() {
        let mut app = ready_to_prestige();
        app.update(Action::OpenPrestige);
        app.update(Action::Confirm);
        app.update(Action::TypeChar('P'));
        app.sync_view(Instant::now());

        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Type  PRESTIGE  to confirm:"), "{frame}");
        assert!(frame.contains("> P___________"), "{frame}");
    }

    /// The property the shared projection exists for: the box cannot quote a price the
    /// panel it was opened from disagrees with.
    #[test]
    fn the_box_and_the_panel_behind_it_quote_one_price() {
        let mut app = ready_to_prestige();
        app.sync_view(Instant::now());
        let panel = whole_frame(&render_to_buffer(&app));
        app.update(Action::OpenPrestige);
        app.sync_view(Instant::now());
        let box_frame = whole_frame(&render_to_buffer(&app));

        let price = denominations(app.view.prestige.cost);
        assert!(panel.contains(&price), "{panel}");
        assert!(box_frame.contains(&price), "{box_frame}");
    }
}

/// The dev menu's tests, gated like the menu.
///
/// A module rather than an attribute per test, for the reason `keymap`'s twin is one:
/// every name below (`Modal::Dev`, `App::with_dev`, `GameState::dev_grant`) is absent
/// from a build with `debug_assertions` off, so these would fail to *compile* under
/// `cargo test --release` rather than skip a feature that is not there.
#[cfg(all(test, debug_assertions))]
mod dev_tests {
    use std::time::Duration;

    use skylode_core::{
        game::GameState,
        material::{Item, Material},
        pickaxe::PickaxeTier,
    };

    use super::tests::{render_to_buffer, session, upgrading, whole_frame};
    use super::*;

    /// A session with the menu enabled, opened a day after the epoch.
    ///
    /// Not at the epoch, unlike [`session`]: the skip row rewinds the offline mark, and
    /// a run whose mark is already at the earliest representable instant has nothing to
    /// rewind — which is a case worth testing, but not the one every other test here
    /// wants to be standing in.
    fn dev_session() -> App {
        let day = std::time::UNIX_EPOCH + Duration::from_secs(86_400);
        App::new(GameState::new(0x5B1_0DE, day)).with_dev(true)
    }

    /// The menu, open, with `row` under the cursor.
    fn on_row(row: DevRow) -> App {
        let mut app = dev_session();
        app.modal = Some(Modal::Dev);
        if let Some(dev) = app.dev.as_mut() {
            dev.row = row;
        }
        app
    }

    /// What the toasts currently say, joined.
    fn said(app: &App) -> String {
        whole_frame(&render_to_buffer(app))
    }

    #[test]
    fn a_plain_session_has_no_menu_and_an_asked_for_one_does() {
        assert!(session().dev.is_none(), "an ordinary run got a dev menu");
        assert!(session().with_dev(false).dev.is_none());
        assert!(session().with_dev(true).dev.is_some());
    }

    #[test]
    fn the_menu_opens_stacks_and_closes() {
        let mut app = dev_session();
        app.update(Action::OpenDevMenu);
        assert_eq!(app.modal, Some(Modal::Dev));

        app.update(Action::CloseModal);
        assert_eq!(app.modal, None);
    }

    #[test]
    fn the_gestures_walk_the_rows_and_turn_the_value_under_the_cursor() {
        let mut app = on_row(DevRow::FreeUpgrades);

        app.update(Action::CursorDown);
        assert_eq!(app.dev.as_ref().map(|dev| dev.row), Some(DevRow::Amount));

        app.update(Action::AdjustRight);
        assert_eq!(app.dev.as_ref().map(DevState::amount), Some(10_000));

        app.update(Action::CursorUp);
        app.update(Action::AdjustRight);
        assert_eq!(app.dev.as_ref().map(|dev| dev.free_upgrades), Some(true));
    }

    /// **The reason the values live in `App` and not in the variant.** Dialling a
    /// million, closing the box to look at the Inventory and coming back is the
    /// workflow; a modal that carried its own state would reset it on the way out.
    #[test]
    fn the_dialled_values_survive_the_box_being_closed() {
        let mut app = on_row(DevRow::Amount);
        app.update(Action::AdjustRight);
        app.update(Action::AdjustRight);
        let dialled = app.dev.as_ref().map(DevState::amount);

        app.update(Action::CloseModal);
        app.update(Action::OpenDevMenu);

        assert_eq!(app.dev.as_ref().map(DevState::amount), dialled);
        assert_eq!(dialled, Some(100_000));
    }

    #[test]
    fn giving_a_material_credits_the_pile_the_row_names() {
        let mut app = on_row(DevRow::Material);
        app.update(Action::AdjustRight);
        app.update(Action::AdjustRight);
        let material = match app.dev.as_ref() {
            Some(dev) => dev.material,
            None => unreachable!("the menu was enabled"),
        };

        app.update(Action::Confirm);

        assert_eq!(
            app.state
                .player()
                .get_inventory()
                .count(Item::Raw(material)),
            1_000
        );
        assert!(said(&app).contains("+1 000"), "{}", said(&app));
    }

    #[test]
    fn giving_everything_credits_all_fifteen_piles() {
        let mut app = on_row(DevRow::Everything);
        app.update(Action::Confirm);

        for material in Material::ALL {
            assert_eq!(
                app.state
                    .player()
                    .get_inventory()
                    .count(Item::Raw(material)),
                1_000,
                "{material:?} was not credited"
            );
        }
    }

    /// A dev level-up is announced in the game's own words, because it goes through the
    /// same [`announce::of`] the tick's events do.
    #[test]
    fn giving_experience_announces_the_levels_it_crosses() {
        let mut app = on_row(DevRow::Experience);
        app.update(Action::Confirm);

        assert!(
            app.state.player().get_level() > 1,
            "a thousand experience bought no level"
        );
        assert!(app.toasts.len() > 1, "only the row's own toast was raised");
        assert!(said(&app).contains("+1 000 xp"), "{}", said(&app));
    }

    #[test]
    fn setting_the_level_moves_the_run_to_it() {
        let mut app = on_row(DevRow::Level);
        for _ in 0..29 {
            app.update(Action::AdjustRight);
        }
        app.update(Action::Confirm);

        assert_eq!(app.state.player().get_level(), 30);
        assert!(said(&app).contains("Level 30"), "{}", said(&app));
    }

    #[test]
    fn granting_charges_fills_the_reserve() {
        let mut app = on_row(DevRow::Charges);
        app.update(Action::AdjustRight);
        app.update(Action::Confirm);

        assert_eq!(app.state.boost_charges(), 2);
    }

    #[test]
    fn setting_the_rank_moves_it_without_wiping_the_run() {
        let mut app = on_row(DevRow::Prestige);
        app.update(Action::AdjustRight);
        app.update(Action::Confirm);

        assert_eq!(app.state.player().get_prestige(), 1);
    }

    #[test]
    fn skipping_time_credits_the_absence_through_the_shipped_accrual() {
        let mut app = on_row(DevRow::SkipTime);
        let before = app
            .state
            .player()
            .get_inventory()
            .count(Item::Raw(app.state.current_mine().kind().common_material()));

        app.update(Action::Confirm);

        let after = app
            .state
            .player()
            .get_inventory()
            .count(Item::Raw(app.state.current_mine().kind().common_material()));
        assert!(after > before, "an hour of auto-mining credited nothing");
        assert!(said(&app).contains("Skipped 1 h"), "{}", said(&app));
    }

    /// A mark already at the epoch has nothing to rewind, and the row says so rather
    /// than claiming a skip that did not happen.
    #[test]
    fn a_skip_with_nothing_behind_it_says_so() {
        let mut app = session().with_dev(true);
        app.modal = Some(Modal::Dev);
        if let Some(dev) = app.dev.as_mut() {
            dev.row = DevRow::SkipTime;
        }

        app.update(Action::Confirm);

        assert!(said(&app).contains("Skipped nothing"), "{}", said(&app));
    }

    /// **The whole of what the toggle buys**: a penniless run climbs the ladder the
    /// Upgrades screen draws, through that screen, with its own cursor and its own key.
    #[test]
    fn free_upgrades_buy_a_rung_the_purse_could_never_afford() {
        let mut app = upgrading(UpgradeTab::Pickaxe).with_dev(true);
        if let Some(dev) = app.dev.as_mut() {
            dev.free_upgrades = true;
        }
        app.cursors.pickaxe_rung = 6;

        app.update(Action::Confirm);

        assert!(
            app.state.player().get_pickaxe().get_tier() > PickaxeTier::Wooden,
            "the free climb bought nothing"
        );
        assert_eq!(
            app.state.player().get_inventory(),
            &skylode_core::inventory::Inventory::new(),
            "a free climb spent something"
        );
    }

    /// It is free, not unlimited: `M` runs to the end of the ladder and stops at the
    /// cap the rules set, which no price was ever enforcing.
    #[test]
    fn free_upgrades_stop_at_the_cap_and_not_at_the_purse() {
        let mut app = upgrading(UpgradeTab::Pickaxe).with_dev(true);
        if let Some(dev) = app.dev.as_mut() {
            dev.free_upgrades = true;
        }

        app.update(Action::BuyMax);
        assert_eq!(
            app.state.player().get_pickaxe().get_tier(),
            PickaxeTier::Netherite
        );

        // A second `M` is refused by the cap in the core's own words — free mode did not
        // make the ladder longer, and `M` asking for one more rung than exists is a
        // question the rules already have an answer to.
        app.toasts = Toasts::new();
        app.update(Action::BuyMax);
        assert!(said(&app).contains("fully upgraded"), "{}", said(&app));
    }

    /// **`Enter` on a rung already owned buys nothing and refuses nothing**, which is the
    /// third of the three outcomes: the two above it are a purchase and a cap, and this
    /// one is a keypress that was simply early.
    #[test]
    fn aiming_a_free_purchase_at_where_you_already_stand_is_not_a_refusal() {
        let mut app = upgrading(UpgradeTab::Pickaxe).with_dev(true);
        if let Some(dev) = app.dev.as_mut() {
            dev.free_upgrades = true;
        }
        app.cursors.pickaxe_rung = 0;

        app.update(Action::Confirm);

        assert_eq!(
            app.state.player().get_pickaxe().get_tier(),
            PickaxeTier::Wooden
        );
        assert!(said(&app).contains("Nothing left to buy"), "{}", said(&app));
    }

    /// The enchant track, free: one level on `Enter`, and up to the world's cap on `M`.
    #[test]
    fn free_upgrades_climb_an_enchant_to_the_worlds_cap() {
        let mut app = upgrading(UpgradeTab::Enchants).with_dev(true);
        if let Some(dev) = app.dev.as_mut() {
            dev.free_upgrades = true;
        }
        let kind = app.cursors.enchant;

        app.update(Action::Confirm);
        let level = |app: &App| app.state.player().get_pickaxe().enchants().get_level(kind);
        assert_eq!(level(&app), 1, "one press bought more than one level");
        assert!(said(&app).contains("Bought"), "{}", said(&app));

        app.update(Action::BuyMax);
        assert_eq!(
            level(&app),
            skylode_core::world::World::Overworld.enchant_cap(),
            "the climb did not stop at the Overworld's cap"
        );
    }

    /// The two mine tracks, free, on the mine the run is standing in.
    #[test]
    fn free_upgrades_climb_both_tracks_of_a_visited_mine() {
        for track in MineTrack::ALL {
            let mut app = upgrading(UpgradeTab::Mines).with_dev(true);
            if let Some(dev) = app.dev.as_mut() {
                dev.free_upgrades = true;
            }
            let standing = app.state.current_mine().kind();
            app.cursors.mine_track = (standing, track);

            app.update(Action::Confirm);

            let level = app.state.mine(standing).map_or(0, |mine| match track {
                MineTrack::Size => mine.get_size_level(),
                MineTrack::Richness => mine.get_richness_level(),
            });
            assert_eq!(level, 1, "{track:?} did not climb");
            assert!(said(&app).contains("level 1"), "{}", said(&app));
        }
    }

    /// `Enter` on the two rows that only hold a value says what the row now reads, rather
    /// than doing nothing at all.
    #[test]
    fn the_value_only_rows_report_themselves_on_confirm() {
        let mut app = on_row(DevRow::FreeUpgrades);
        app.update(Action::Confirm);
        assert!(said(&app).contains("Free upgrades off"), "{}", said(&app));

        let mut app = on_row(DevRow::Amount);
        app.update(Action::Confirm);
        assert!(said(&app).contains("Amount 1 000"), "{}", said(&app));
    }

    /// `←` turns the value down, which is not the same code path as `→`.
    #[test]
    fn the_left_gesture_turns_the_value_down() {
        let mut app = on_row(DevRow::Amount);
        app.update(Action::AdjustLeft);
        assert_eq!(app.dev.as_ref().map(DevState::amount), Some(100));
    }

    /// **An unreachable state, answered rather than ignored.** `keymap` cannot emit the
    /// key that stacks this modal without a `DevState` behind it — but a `Modal::Dev`
    /// with no menu would otherwise capture every key and never draw anything, which is
    /// a locked terminal rather than a bug report.
    #[test]
    fn a_menu_stacked_without_a_state_closes_itself() {
        let mut app = session();
        app.modal = Some(Modal::Dev);

        app.update(Action::CursorDown);

        assert_eq!(app.modal, None);
    }

    /// Free mode still refuses what the *rules* refuse — an unvisited mine is not a
    /// purchase that ore was standing in the way of.
    #[test]
    fn a_free_purchase_is_still_refused_on_a_mine_the_run_never_entered() {
        let mut app = upgrading(UpgradeTab::Mines).with_dev(true);
        if let Some(dev) = app.dev.as_mut() {
            dev.free_upgrades = true;
        }
        app.cursors.mine_track = (MineKind::Coal, MineTrack::Size);

        app.update(Action::Confirm);

        assert!(
            app.state.mine(MineKind::Coal).is_none(),
            "a grid was minted"
        );
        assert!(said(&app).contains("enter the Coal mine"), "{}", said(&app));
    }

    /// **The marker is present exactly when the menu is**, and it lands in the gap after
    /// the six tabs rather than over one of them: `DEV FREE` was the first draft and it
    /// ate three letters of `6 Levels`, and `FREE` still abutted it.
    #[test]
    fn the_tab_row_carries_the_marker_only_when_the_menu_exists() {
        assert!(
            !said(&session()).contains(dev::MARKER),
            "an ordinary run said {}",
            dev::MARKER
        );

        let frame = said(&dev_session());
        assert!(
            frame.contains(&format!("6 Levels {}", dev::MARKER)),
            "the marker did not land in the gap after the last tab\n{frame}"
        );
    }

    /// Turning the free toggle **announces itself**, which is what makes the marker's
    /// colour a reminder rather than the only notice the mode ever gives.
    #[test]
    fn flipping_the_free_toggle_announces_it() {
        let mut app = on_row(DevRow::FreeUpgrades);

        app.update(Action::AdjustRight);
        assert!(said(&app).contains("Free upgrades on"), "{}", said(&app));

        app.toasts = Toasts::new();
        app.update(Action::AdjustLeft);
        assert!(said(&app).contains("Free upgrades off"), "{}", said(&app));

        // Another row's adjust says nothing — the announcement belongs to the mode, not
        // to the gesture. Counted rather than read off the frame: the open menu draws the
        // words `Free upgrades` as a row label, so the frame cannot tell a toast about
        // the mode from the row that sets it.
        let mut app = on_row(DevRow::Amount);
        app.update(Action::AdjustRight);
        assert_eq!(app.toasts.len(), 0, "turning a value announced something");
    }

    #[test]
    fn the_open_menu_is_drawn_over_the_screen_behind_it() {
        let mut app = dev_session();
        app.update(Action::OpenDevMenu);
        let frame = said(&app);
        assert!(frame.contains("Dev menu"), "{frame}");
        assert!(frame.contains("Free upgrades"), "{frame}");
    }
}
