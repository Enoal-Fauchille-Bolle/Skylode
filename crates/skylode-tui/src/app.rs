//! The application: state, the render loop, and the reducer that mutates it.
//!
//! `App` owns **UI state only** — which tab is open, which modal is stacked, the
//! live toasts. It deliberately owns no game rules: those belong to
//! `skylode-core`, and what the screens draw arrives as a flat [`View`] snapshot.
//! Keeping the split means a list cursor never leaks into a save file, and the
//! core stays testable without a terminal.

use std::time::Instant;

use color_eyre::Result;
use ratatui::{
    Frame, Terminal,
    backend::Backend,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    widgets::Tabs,
};
use skylode_core::{
    game::GameState,
    material::{Item, Material},
    mine::Mine,
    mine_kind::MineKind,
    tunables::RAW_PER_COMPRESSED,
};

use crate::{
    action::Action,
    config::Config,
    cursor::{self, Cursors},
    event::{Event, Events},
    format::grouped,
    keymap,
    overlay::{Conversion, Modal, compression, help, too_small},
    screen::Screen,
    theme,
    toast::{TOAST_TTL, Toasts},
    view::View,
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
    /// home: `main` builds one and hands it over, and phase 7's tick will mutate it
    /// from inside the loop. The boundary this crate keeps is not "the front-end
    /// may not hold game state" — it is that the front-end holds no game *rules*.
    /// Every field of `GameState` is private and every mutation goes through a
    /// method that can refuse.
    pub state: GameState,
    /// What the screens render — [`View::from_state`]'s answer, cached.
    ///
    /// Rebuilt by [`sync_view`](App::sync_view) before each draw rather than inside
    /// `render`, for two reasons. `render` takes `&self` and stays a pure read, which
    /// is what lets a test draw an `App` it does not own; and the projection still
    /// rebuilds `View::sample`'s fixture for the three screens phases 6-7 have not
    /// wired, which is worth doing when the state changes and not thirty times a
    /// second.
    pub view: View,
    /// Where the player is pointing on each list — front-end state, never the run's.
    ///
    /// Held beside [`state`](App::state) rather than inside it, because a highlighted
    /// row is not something a save should carry and not something the rules may
    /// consult. [`View::from_state`] reads both to build one snapshot.
    pub cursors: Cursors,
    /// Front-end preferences — read while drawing, edited by Settings (phase 7).
    pub config: Config,
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
        // actually is rather than the top of the list.
        let cursors = Cursors::new(state.current_mine().kind());
        let view = View::from_state(&state, cursors);
        Self {
            should_quit: false,
            screen: Screen::Mine,
            modal: None,
            toasts: Toasts::new(),
            state,
            view,
            cursors,
            config: Config::default(),
        }
    }

    /// Rebuilds the read model from the run.
    ///
    /// Called once per frame, before drawing, and unconditional. Redraw-on-change is
    /// phase 7's, where the 20 tps tick makes most frames identical to the last and a
    /// guard here starts earning its keep; today a session only changes when a key is
    /// pressed, so the projection runs about as often as it would anyway.
    fn sync_view(&mut self) {
        self.view = View::from_state(&self.state, self.cursors);
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
            // Before the draw, not after the event: the first frame must show the
            // run as it stands, and `new` has already projected it once so this is
            // the identity on the opening pass.
            self.sync_view();
            terminal.draw(|frame| self.render(frame))?;

            match events.next()? {
                Event::Tick => self.on_tick(),
                Event::Key(key) => {
                    if let Some(action) = keymap::resolve(&self, key) {
                        self.update(action);
                    }
                }
                // Nothing to do: ratatui lays out against the new size on the
                // next draw, which the loop is about to perform anyway.
                Event::Resize => {}
            }
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
    /// screen behind the box. The five overlays phases 6 and 7 still owe inherit this
    /// seam rather than re-deriving it, which is why the split lives here in one line
    /// and not as a condition repeated in each arm.
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
            Action::ShowToast(text) => self.toasts.push(text, TOAST_TTL),
            // `?` stacks Help over the current screen; `Esc`/`?` clear it. The
            // keymap only emits `OpenHelp` when nothing is stacked, so this never
            // buries one modal under another.
            Action::OpenHelp => self.modal = Some(Modal::Help),
            Action::CloseModal => self.modal = None,
            // The list gestures are decoded without a screen in mind, so this is
            // where one is chosen. Mines and Inventory answer today; phases 6-7 add
            // arms.
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
            Action::Confirm => {
                if self.screen == Screen::Mines {
                    self.enter_selected_mine();
                }
            }
            // Nothing to adjust to its maximum outside the spinner, which
            // `update_modal` has already answered for.
            Action::AdjustMax => {}
            Action::Compress => self.open_conversion(Conversion::Compress),
            Action::Decompress => self.open_conversion(Conversion::Decompress),
        }
    }

    /// Offers `action` to the stacked modal, answering whether it took it.
    ///
    /// **Only the compression dialog answers anything**, because it is the only modal
    /// with a value in it. Help swallows keys in [`keymap`] and never reaches here
    /// with a gesture at all; `Esc` deliberately falls through to
    /// [`Action::CloseModal`] in the main `match`, so closing a modal stays one
    /// implementation for every modal there will ever be.
    ///
    /// Returning a `bool` rather than an `Option<Action>` to re-dispatch: a modal
    /// either consumed the gesture or did not, and translating one gesture into
    /// another would give the reducer a second dispatch path to reason about.
    fn update_modal(&mut self, action: &Action) -> bool {
        let Some(Modal::Compress {
            material,
            direction,
            units,
        }) = self.modal
        else {
            return false;
        };

        match action {
            // `saturating_sub` on the way down and a clamp on the way up: the floor
            // is 1, since a conversion of nothing is not something the dialog should
            // be able to offer, and `Esc` is how a player who changed their mind
            // leaves.
            Action::AdjustLeft => self.set_spinner(material, direction, units.saturating_sub(1)),
            Action::AdjustRight => self.set_spinner(material, direction, units.saturating_add(1)),
            // `a` — *all*. Asking for more than the pile holds and letting the clamp
            // answer, rather than reading the ceiling twice.
            Action::AdjustMax => self.set_spinner(material, direction, u32::MAX),
            Action::Confirm => self.apply_conversion(material, direction, units),
            _ => return false,
        }
        true
    }

    /// Moves the spinner to `requested`, clamped into what the pile can actually
    /// convert.
    ///
    /// **The ceiling is re-read from the inventory on every step** rather than stored
    /// beside the count when the dialog opened. It costs a division, and it means the
    /// bound cannot go stale — phase 7's tick credits loot while a modal is up, so a
    /// ceiling captured at opening time would be wrong by the second keypress.
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
        self.toasts.push(refusal, TOAST_TTL);
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

        let message = match outcome {
            Ok(()) => {
                let (item, amount) = gained;
                format!("+{} {item}", grouped(amount))
            }
            Err(refusal) => refusal.to_string(),
        };
        self.toasts.push(message, TOAST_TTL);
        self.modal = None;
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
            _ => {}
        }
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
            Err(refusal) => self.toasts.push(refusal.to_string(), TOAST_TTL),
        }
    }

    /// The heartbeat. Expires toasts today; drives the game tick from phase 7.
    fn on_tick(&mut self) {
        self.toasts.prune(Instant::now());
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
    use std::time::Duration;

    use ratatui::{
        backend::TestBackend,
        buffer::Buffer,
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    };
    use skylode_core::game::Input;

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
    fn showing_a_toast_queues_it() {
        let mut app = session();
        assert!(app.toasts.is_empty());
        app.update(Action::ShowToast("Mine refilled".to_owned()));
        assert_eq!(app.toasts.len(), 1);
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
        app.update(Action::ShowToast("Mine refilled".to_owned()));
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Mine refilled"), "{frame}");
    }

    #[test]
    fn the_heartbeat_expires_a_toast_once_its_moment_has_passed() {
        // **`on_tick` is called, not stepped around.** The test this replaces was
        // named for the heartbeat and reached straight for `toasts.prune`, so the one
        // line it was about — `on_tick`'s body, the thing `Event::Tick` actually runs
        // — was never executed by anything. A test can assert the right outcome and
        // still miss the code that is supposed to produce it.
        //
        // `on_tick` reads `Instant::now()` itself, so the clock is not the test's to
        // choose: it ticks once against the live clock to prove the heartbeat spares
        // a live toast, then prunes past the deadline by hand for the other half.
        let mut app = session();
        app.update(Action::ShowToast("Excavator!".to_owned()));
        assert_eq!(app.toasts.len(), 1);

        // A tick right now expires nothing: the toast has three seconds to live.
        app.on_tick();
        assert_eq!(app.toasts.len(), 1, "the heartbeat ate a live toast");

        app.toasts
            .prune(Instant::now() + TOAST_TTL + Duration::from_millis(1));
        assert_eq!(app.toasts.len(), 0, "the toast outlived its TTL");
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
        app.update(Action::ShowToast("Mine refilled".to_owned()));
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
        // Two thousand ticks is comfortably past the hundred raw one unit needs.
        for _ in 0..2_000 {
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
    /// this. What can is phase 7's tick, which will credit and spend while a modal is
    /// up. The dialog closes either way — leaving it would invite the player to press
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
}
