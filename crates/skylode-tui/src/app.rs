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
    Frame, Terminal,
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::Tabs,
};
use skylode_core::{
    economy::{self, Affordability, Shortfall},
    enchant::EnchantType,
    game::{GameState, Input},
    material::{Item, Material},
    mine::Mine,
    mine_kind::MineKind,
    tunables::{LEVEL_CAP, RAW_PER_COMPRESSED, TICKS_PER_SECOND},
    upgrade,
};

use crate::{
    action::Action,
    announce,
    config::Config,
    cursor::{self, Cursors, MineTrack, UpgradeTab},
    event::{Event, Events},
    format::{denominations, grouped, roman, rung_label},
    keymap,
    overlay::{Conversion, Modal, compression, dip, help, too_small},
    screen::Screen,
    theme,
    toast::{TOAST_TTL, Toasts, Tone},
    view::{CompressHint, UpgradeDetail, View},
};

/// The widest the interface is ever drawn, whatever the terminal offers.
///
/// **Twice the counted frame**, and that is the whole justification: the wireframes
/// in UI-EN.md §5 are 80 columns of *deliberately dense* text, so at 240 columns a
/// detail pane would be a hundred columns of whitespace with a forty-column
/// sentence adrift in it. Past this width the surplus becomes margin either side
/// rather than more line to cross with the eye.
const MAX_WIDTH: u16 = 2 * too_small::MIN_WIDTH;

/// The tallest, for the same reason and by the same arithmetic.
///
/// This one bites less often — a list genuinely uses every row it is given — but
/// the Mine screen's grid is a game constant, so a 90-row terminal would strand it
/// in the middle of an enormous empty box.
const MAX_HEIGHT: u16 = 2 * too_small::MIN_HEIGHT;

/// One simulation step, derived from the core's own tick rate.
///
/// **Derived and not written down**, because 20 tps is a game rule
/// (`docs/MECHANICS.md`) and a front-end that spelled `50` would be a second copy of
/// it — one that could be edited without the balance pass noticing. The division is
/// exact for any rate that divides a second evenly, and nanoseconds are what make it
/// exact at all: at 30 tps, milliseconds would floor to 33 and lose 1% of the day.
const SIM_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / TICKS_PER_SECOND);

/// The shortest gap between two draws — a **ceiling on the redraw rate**, not a
/// cadence.
///
/// The loop draws when something changed *and* this much time has passed, so a burst
/// of held keys cannot ask the terminal for two hundred frames a second. It is
/// deliberately shorter than [`SIM_PERIOD`]: today the simulation is the only thing
/// that changes the screen, so the real rate is 20 fps and this ceiling never binds —
/// but the proc flash (two stages of ~100 ms, `docs/UI.md` §7) changes the screen
/// *between* ticks, and it is the reason the two clocks are separate now rather than
/// separated later.
/// **Input is exempt**: a key that meant something draws on the spot, because the
/// only burst it can produce is bounded by the terminal's own repeat rate, and 33 ms
/// of latency in the one place the player is looking is worse than a frame nobody
/// asked for.
const FRAME_PERIOD: Duration = Duration::from_millis(33);

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

/// The whole front-end state.
#[derive(Debug)]
pub struct App {
    /// Set by [`Action::Quit`]; the loop reads it and stops.
    pub should_quit: bool,
    /// The tab currently on screen.
    pub screen: Screen,
    /// The modal stacked over it, if any.
    ///
    /// **It carries the modal's own state, not just which one is up**, which is why
    /// [`Modal::Compress`] has fields: a dialog with a value in it has nowhere else to
    /// keep that value where "no dialog" and "a dialog reading zero" stay distinct.
    /// [`keymap`] gives whatever is here first refusal on every key, and
    /// [`update`](App::update) gives it first refusal on every gesture.
    pub modal: Option<Modal>,
    /// Live announcements, drawn over everything.
    pub toasts: Toasts,
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
    /// When the next simulation step falls due.
    ///
    /// **A deadline, not a countdown**, which is what makes the 20 tps rate survive a
    /// late wake-up: [`advance`](App::advance) runs steps *until* this passes `now`
    /// and adds [`SIM_PERIOD`] to it each time, so a frame that took 80 ms runs the
    /// step it owed and stays on the same grid. A remaining-time counter decremented
    /// by the elapsed time would instead drift by whatever each frame overshot, and a
    /// run's pace would depend on how busy the machine was.
    next_tick: Instant,
    /// The earliest the next draw may happen — [`FRAME_PERIOD`]'s ceiling.
    next_frame: Instant,
    /// Whether anything has changed since the last draw.
    ///
    /// *Redraw on change* in the one form the front-end can answer cheaply: raised by
    /// a key that was acted on, a resize, and any simulation step that ran. The last
    /// of those is why this is not a saving today — a step always runs, because the
    /// auto-miner credits on every one of them and the Haul strip would go stale if
    /// it did not. What the flag buys now is that a *quiet* loop — no ticks due, no
    /// input — asks the terminal for nothing at all.
    dirty: bool,
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
        let cursors = Cursors::new(
            state.current_mine().kind(),
            upgrade::position(&upgrade::ladder(), state.player().get_pickaxe()),
            state.player().get_level(),
        );
        let view = View::from_state(&state, cursors, None);
        // Both clocks start due, so the first pass through the loop draws and the
        // first step falls in one period rather than one period from whenever the
        // session happened to be built.
        let now = Instant::now();
        Self {
            should_quit: false,
            screen: Screen::Mine,
            modal: None,
            toasts: Toasts::new(),
            state,
            view,
            cursors,
            refused: None,
            config: Config::default(),
            next_tick: now + SIM_PERIOD,
            next_frame: now,
            dirty: true,
            last_mine_key: None,
            mine_key_edge: None,
        }
    }

    /// Rebuilds the read model from the run.
    ///
    /// Called before drawing, and only when a draw is actually about to happen: the
    /// guard is [`dirty`](App#structfield.dirty), on the caller's side, so a pass that
    /// asks the terminal for nothing does not project a snapshot nobody reads. Which
    /// is what keeps this affordable now that a 20 tps tick changes the run whether
    /// the player touches anything or not.
    fn sync_view(&mut self) {
        self.view = View::from_state(&self.state, self.cursors, self.refused.as_ref());
    }

    /// Draws, then blocks for the next event, until asked to quit.
    ///
    /// Rendering happens *before* waiting so the first frame appears immediately
    /// rather than after the first keypress. Blocking on `events.next()` is what
    /// keeps the app at zero CPU while idle; the tick guarantees we still wake up
    /// often enough to expire toasts.
    ///
    /// **Generic over both of its collaborators, so that the loop itself can be
    /// tested.** It used to take a `DefaultTerminal` — ratatui's alias for
    /// `Terminal<CrosstermBackend<Stdout>>` — and a concrete
    /// [`EventHandler`](crate::event::EventHandler), and
    /// between them they made this function unreachable from a test: the backend
    /// writes to the real stdout, and the handler's thread dies the moment it polls
    /// a terminal that is not there. Everything else in the crate is exercised
    /// through ratatui's own `TestBackend`; these two parameters are what let the
    /// loop join it.
    ///
    /// Generics rather than `dyn`: both types are known at every call site, so the
    /// compiler emits one specialised copy per pair (*monomorphisation*) and the
    /// indirection costs nothing at runtime. `main` still passes the real terminal
    /// and the real handler, and neither has to change.
    ///
    /// The `where` clause is what `?` needs. Since ratatui 0.30 a backend names its
    /// own error type rather than always being `io::Error`, and `color_eyre::Report`
    /// can only absorb one that is a `std::error::Error` it can carry across threads
    /// and outlive the frame. Both real backends satisfy it, including
    /// `TestBackend`, whose error is [`Infallible`](core::convert::Infallible) — a
    /// type with no values, so the conversion is one that provably never runs.
    /// The terminal is borrowed, not consumed: `run` has no business dropping it —
    /// `main` restores the screen afterwards and needs it alive to do so — and a
    /// caller that could not look at the terminal again after the loop returned could
    /// not read the last frame the player saw.
    pub fn run<B, E>(mut self, terminal: &mut Terminal<B>, events: E) -> Result<()>
    where
        B: Backend,
        B::Error: std::error::Error + Send + Sync + 'static,
        E: Events,
    {
        while !self.should_quit {
            let now = Instant::now();
            // Before the wait, not after: the first frame must show the run as it
            // stands rather than appearing on the first keypress. `new` starts both
            // flags due, so the opening pass always draws.
            if self.dirty && now >= self.next_frame {
                self.sync_view();
                terminal.draw(|frame| self.render(frame))?;
                self.dirty = false;
                self.next_frame = now + FRAME_PERIOD;
            }

            match events.next()? {
                // **Nothing.** The heartbeat's whole job is to end the block above's
                // wait so that `advance` below gets to look at the clock; how many
                // beats arrived is not a quantity anything here counts. That is the
                // difference between a heartbeat and a cadence, and it is what lets
                // the simulation keep 20 tps whatever rate this channel runs at.
                Event::Tick => {}
                Event::Key(key) => {
                    if let Some(action) = keymap::resolve(&self, key) {
                        self.update(action);
                        // Only a key that *meant* something redraws. An unbound key
                        // changed nothing, and a frame that redraws the same buffer
                        // is work the terminal has to undo.
                        self.dirty = true;
                        // **And it redraws now, not at the next allowed frame.** The
                        // ceiling exists to stop the *simulation* from asking for two
                        // hundred frames a second; input cannot ask for more than the
                        // keyboard repeats, and a tab that appears 33 ms after the key
                        // is latency the player feels in the one place they are
                        // looking. Bursts stay bounded because a terminal's repeat
                        // rate is.
                        self.next_frame = now;
                    }
                }
                // ratatui lays out against the new size on the next draw; all this
                // has to do is make sure there is one.
                Event::Resize => self.dirty = true,
            }

            // After the event and not before: a key pressed this pass should reach
            // the step it belongs to, not the one after it.
            self.advance(Instant::now());
        }
        Ok(())
    }

    /// Applies one decoded intent.
    ///
    /// This is the reducer, and it is the reason [`Action`] exists: it takes no
    /// `KeyEvent` and touches no terminal, so every transition below is a plain
    /// unit test. The `match` is exhaustive, so a new [`Action`] variant cannot be
    /// added without deciding what it does here.
    ///
    /// **A modal is offered the gesture before the screen is, and that ordering is
    /// the rule.** A modal captures the keyboard — [`keymap`] already gives it first
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
            Action::Quit => self.should_quit = true,
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
            Action::Compress => self.open_conversion(Conversion::Compress),
            Action::Decompress => self.open_conversion(Conversion::Decompress),
            Action::GoCompress => self.walk_to_the_refused_pile(),
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
    /// a caret's side. Help swallows keys in [`keymap`] and never reaches here with a
    /// gesture at all; `Esc` deliberately falls through to [`Action::CloseModal`] in
    /// the main `match`, so closing a modal stays one implementation for every modal
    /// there will ever be.
    ///
    /// Returning a `bool` rather than an `Option<Action>` to re-dispatch: a modal
    /// either consumed the gesture or did not, and translating one gesture into
    /// another would give the reducer a second dispatch path to reason about.
    fn update_modal(&mut self, action: &Action) -> bool {
        match self.modal {
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
            _ => false,
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
    /// A refusal is toasted verbatim: [`CoreError`](skylode_core::error::CoreError)
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
        match self.cursors.upgrade_tab {
            UpgradeTab::Pickaxe => self.buy_pickaxe_chain(reach),
            UpgradeTab::Enchants => self.buy_enchant_levels(reach),
            UpgradeTab::Mines => self.buy_mine_track(reach),
        }
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
                let label = format!("{} {what} {}", kind.name(), level + 1);
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
    /// **The screen is chosen here and not in [`keymap`], which is the whole shape of
    /// [`Action`]'s list gestures.** `↑` decodes to [`Action::CursorUp`] without
    /// knowing what it will move, because the keymap has no access to the run; which
    /// cursor that is lands where the state is, and that is here.
    ///
    /// Both arms delegate to [`cursor::step_in`], so the *lists clamp, rings wrap*
    /// rule has one implementation rather than one per screen. A screen with no list
    /// does nothing, which is why this is a `match` with a catch-all rather than a
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
            // The ladder is `1..=LEVEL_CAP`, contiguous and one-based, so the clamp is
            // over indices and the level is that index plus one. Routed through
            // `step_index` rather than done here with a `+ delta`, so *lists clamp* has
            // one implementation on this screen too — the failure mode being a `↑` on
            // level 1 that wrapped the roadmap to the cap.
            Screen::Levels => {
                let index = cursor::step_index(LEVEL_CAP as usize, self.level_index(), delta);
                self.cursors.level = index.saturating_add(1) as u32;
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
    /// [`CoreError`](skylode_core::error::CoreError)'s own wording already names both
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
    fn advance(&mut self, now: Instant) {
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
        }

        self.toasts.prune(now);
        // A step that ran changed something by construction — the auto-miner credits
        // on every one — so this is `steps > 0` rather than a test of the events,
        // which most steps produce none of.
        self.dirty |= steps > 0;
    }

    /// Paints one frame: tab bar, active screen, then the overlays on top.
    ///
    /// Order is the layering: overlays draw last precisely so they cover the
    /// screen rather than being covered by it.
    fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        // The terminal-too-small filter, in front of everything (UI-EN.md §6.2).
        // It is not a screen and not a modal: below the 80×24 budget it replaces
        // the whole frame regardless of which tab or overlay is up, and yields it
        // back untouched once the window grows, because it reads no state. Drawing
        // it here — before the tab bar even splits the area — is what "a filter,
        // not a state with edges" means in code.
        if !too_small::fits(area) {
            too_small::render(frame, area);
            return;
        }

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
        self.toasts.render(frame, area);
        if let Some(modal) = self.modal {
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
                    material,
                    direction,
                    units,
                ),
                // The opposite choice to the dialog above, and for the opposite reason:
                // the dip is about a purchase whose numbers the player has *already
                // read* in the pane behind, so it draws that same projection rather
                // than a second reading of the run that could disagree with it.
                Modal::Dip { buy, .. } => {
                    if let UpgradeDetail::Pickaxe(detail) = &self.view.upgrades.pickaxe.detail {
                        dip::render(frame, area, detail, buy);
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
    }
}

#[cfg(test)]
mod tests {

    use ratatui::{
        backend::TestBackend,
        buffer::Buffer,
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    };
    use skylode_core::{game::Input, pickaxe::PickaxeTier, save};

    use super::*;

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
    fn session() -> App {
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
    fn render_to_buffer(app: &App) -> Buffer {
        render_to_sized_buffer(app, 80, 24)
    }

    /// The same, at an arbitrary size — for the too-small filter, whose whole job
    /// is what happens below 80×24.
    fn render_to_sized_buffer(app: &App, width: u16, height: u16) -> Buffer {
        let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        if let Err(infallible) = terminal.draw(|frame| app.render(frame)) {
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
    fn whole_frame(buffer: &Buffer) -> String {
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
        assert!(!app.should_quit);
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
        app.update(Action::Quit);
        assert!(app.should_quit);
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
    fn a_cramped_terminal_shows_the_filter_instead_of_the_open_screen() {
        // Sitting on a non-default tab, under the budget: the filter must win over
        // whatever was up, drawing its message and none of the tab bar.
        let mut app = session();
        app.update(Action::SelectScreen(2));
        let frame = whole_frame(&render_to_sized_buffer(&app, 54, 18));
        assert!(frame.contains("Skylode needs 80 x 24"), "{frame}");
        assert!(
            !frame.contains("2 Inventory"),
            "the tab bar leaked through: {frame}"
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

    #[test]
    fn a_step_expires_a_toast_once_its_moment_has_passed() {
        // **`advance` is called, not stepped around**, and now it can be: the instant
        // is an argument, so both halves — the toast that survives and the toast that
        // does not — are the test's to choose rather than the machine's.
        let start = Instant::now();
        let mut app = session();
        app.toasts
            .push_at("Excavator!".to_owned(), Tone::Success, TOAST_TTL, start);

        app.advance(start + SIM_PERIOD);
        assert_eq!(app.toasts.len(), 1, "a step ate a live toast");

        app.advance(start + TOAST_TTL + Duration::from_millis(1));
        assert_eq!(app.toasts.len(), 0, "the toast outlived its TTL");
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
        // which is precisely where the ~30 fps ceiling will matter once the proc flash
        // animates there.
        let mut app = session();
        app.dirty = false;
        let first = step_due(&app);

        app.advance(first - Duration::from_millis(1));
        assert!(!app.dirty, "a pass with no step due asked for a frame");

        app.advance(first);
        assert!(app.dirty, "a step ran without asking for a frame");
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

    /// An event source that reads from a script instead of from a terminal.
    ///
    /// The whole reason [`App::run`] is generic. It hands out the scripted events in
    /// order and then returns an error, which is how the loop is made to stop even if
    /// the script never quits: a real `EventHandler` blocks forever waiting for a key
    /// that a test will never press, so "the script ran out" has to be a *failure*
    /// rather than a silence. Every test below asserts on the state after `run`
    /// returns, so which of the two ways it ended is checked explicitly.
    ///
    /// `Cell` and not `&mut self`: [`Events::next`] takes `&self` — the real receiver
    /// needs no exclusive borrow — so the cursor has to be interior-mutable. `Cell`
    /// rather than `RefCell` because a `usize` is `Copy` and there is nothing to
    /// borrow, which makes the read a plain load and not a runtime borrow check.
    struct Script {
        events: Vec<Event>,
        next: std::cell::Cell<usize>,
    }

    impl Script {
        fn new(events: Vec<Event>) -> Self {
            Self {
                events,
                next: std::cell::Cell::new(0),
            }
        }
    }

    impl Events for Script {
        fn next(&self) -> Result<Event> {
            let index = self.next.get();
            self.next.set(index + 1);
            self.events
                .get(index)
                .copied()
                .ok_or_else(|| color_eyre::eyre::eyre!("the script ran out"))
        }
    }

    /// A key press with no modifiers, as the event source would report it.
    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// Runs the real loop over `events`, into an off-screen 80×24 terminal.
    ///
    /// Hands back the terminal too, so a test can read what the *last* frame drew —
    /// the loop draws before every wait, so the buffer after `run` is the frame the
    /// player was looking at when they quit.
    fn run_script(events: Vec<Event>) -> (Result<()>, Buffer) {
        let mut terminal = match Terminal::new(TestBackend::new(80, 24)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        let result = session().run(&mut terminal, Script::new(events));
        (result, terminal.backend().buffer().clone())
    }

    #[test]
    fn the_loop_draws_before_it_waits() {
        // The first frame must be on screen *before* the first event is asked for,
        // or the player stares at a blank terminal until they touch a key. Asserted
        // by giving the loop a script that quits on its very first event: if drawing
        // came second, nothing would ever have been painted.
        let (result, buffer) = run_script(vec![key(KeyCode::Char('q'))]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("1 Mine"), "nothing was drawn: {frame}");
    }

    #[test]
    fn a_key_is_decoded_and_applied_before_the_next_frame() {
        // The loop's real job: key → `keymap::resolve` → `update` → redraw. Two tab
        // presses then a quit, and the frame left on screen has to be the third
        // screen of the ring — proof the keys went through the reducer and that the
        // redraw happened after them rather than before.
        let (result, buffer) = run_script(vec![
            key(KeyCode::Tab),
            key(KeyCode::Tab),
            key(KeyCode::Char('q')),
        ]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("Inventory"), "{frame}");
        assert!(
            frame.contains("Compressible now"),
            "not on Inventory: {frame}"
        );
    }

    #[test]
    fn a_key_nothing_is_bound_to_leaves_the_session_alone() {
        // `resolve` returns `None` and the loop must simply go round again — not
        // quit, not panic, not swallow the next event.
        let (result, buffer) = run_script(vec![key(KeyCode::Char('z')), key(KeyCode::Char('q'))]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        assert!(whole_frame(&buffer).contains("Haul"), "the screen moved");
    }

    #[test]
    fn a_tick_and_a_resize_both_go_round_the_loop_without_changing_the_screen() {
        // `Tick` runs the heartbeat and `Resize` does nothing at all — ratatui lays
        // out against the new size on the next draw, which the loop is about to do
        // anyway. Both must still reach the quit behind them, which is what fails if
        // either arm ever starts returning early.
        let (result, buffer) = run_script(vec![
            Event::Tick,
            Event::Resize,
            Event::Tick,
            key(KeyCode::Char('q')),
        ]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        assert!(whole_frame(&buffer).contains("Haul"), "the screen moved");
    }

    #[test]
    fn a_dead_event_source_stops_the_loop_instead_of_spinning() {
        // The other way out. A real `EventHandler` whose thread has died closes the
        // channel, and `recv` then fails forever — so the `?` on `events.next()` has
        // to end the loop rather than let it spin on an error it ignores. The script
        // reproduces that by running out.
        let (result, _) = run_script(vec![Event::Tick]);
        assert!(result.is_err(), "the loop kept going past a dead source");
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
    fn the_mines_cursor_walks_the_list_and_stops_at_both_ends() {
        let mut app = browsing_mines();
        // A fresh run stands in the Stone mine, which is row zero.
        assert_eq!(app.cursors.mine, MineKind::Stone);

        app.update(Action::CursorDown);
        assert_eq!(app.cursors.mine, MineKind::Coal);
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.mine, MineKind::Stone);

        // Off the top: it stops rather than wrapping to the End mine. Lists clamp,
        // rings wrap — a `↑` that jumped across the whole game would be a surprise.
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.mine, MineKind::Stone);

        // And off the bottom, walked the whole way to prove the clamp is the list's
        // length rather than a number written down here.
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
    fn the_material_cursor_walks_the_table_and_stops_at_both_ends() {
        let mut app = browsing_inventory();
        // Nothing in the run says which material the player is looking at, so the
        // table opens at its first row.
        assert_eq!(app.cursors.material, Material::Stone);

        app.update(Action::CursorDown);
        assert_eq!(app.cursors.material, Material::Coal);
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.material, Material::Stone);

        // Lists clamp, rings wrap — the same rule the Mines list keeps, and here it
        // is kept by the same helper rather than by a second copy of the arithmetic.
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.material, Material::Stone);

        // Walked the whole way, so the clamp is the table's length and not a number
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
        app.sync_view();

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
        app.sync_view();
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
        app.sync_view();
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("never entered"), "{frame}");
    }

    // --- The Upgrades screen ---

    /// A session on the Upgrades tab, on the sub-tab named.
    fn upgrading(tab: UpgradeTab) -> App {
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

    /// **The sub-tab ring wraps, and the lists inside it clamp.** Two rules in one
    /// screen, which is exactly why they are two functions — [`UpgradeTab::next`] and
    /// [`cursor::step_index`] — rather than one helper with a flag.
    #[test]
    fn the_sub_tabs_are_a_ring_and_the_rows_inside_them_are_not() {
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

        // The ladder, by contrast, stops. A fresh run stands on rung 0, so `↑` has
        // nowhere to go and must not land on a maxed Netherite pickaxe.
        app.cursors.upgrade_tab = UpgradeTab::Pickaxe;
        app.update(Action::CursorUp);
        assert_eq!(app.cursors.pickaxe_rung, 0);
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
        app.sync_view();
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
        app.sync_view();

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
        app.sync_view();
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
        app.sync_view();
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
        app.sync_view();
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
        app.sync_view();
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Claimed Lv"), "{frame}");
        assert!(
            !frame.contains("levels"),
            "a single bundle was pluralised: {frame}"
        );

        // And again, against a ladder with nothing on it.
        app.toasts = Toasts::new();
        app.update(Action::ClaimAll);
        app.sync_view();
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

    /// The cursor walks the ladder, clamps at both ends, and `Home` brings it back.
    #[test]
    fn the_roadmap_cursor_clamps_at_both_ends_and_home_returns_to_the_player() {
        let mut app = with_rewards_waiting();
        let here = app.state.player().get_level();
        assert_eq!(app.cursors.level, 1, "the session opened above level 1");

        app.update(Action::CursorUp);
        assert_eq!(
            app.cursors.level, 1,
            "the roadmap wrapped past its first rung"
        );

        for _ in 0..LEVEL_CAP + 10 {
            app.update(Action::CursorDown);
        }
        assert_eq!(app.cursors.level, LEVEL_CAP, "the roadmap ran past its cap");

        app.update(Action::JumpToCurrent);
        assert_eq!(app.cursors.level, here);
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
        app.sync_view();
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Wooden Eff I wants"), "{frame}");

        // Move to any other pile and the note goes with it: a price in Stone printed
        // beside the Coal row would attach a number to the wrong thing.
        app.cursors.material = Material::Coal;
        app.sync_view();
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
        app.sync_view();
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
            (MineTrack::Size, "Stone size 1"),
            (MineTrack::Richness, "Stone richness 1"),
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
}
