//! The session: who owns the terminal, and what the player is looking at.
//!
//! [`App`] is *the game* — a run, the screens over it, the toasts. A session is the
//! thing above it: the loop that draws, waits and steps, and — once phase 8's state
//! machine lands — the title screen, the recovery frames and the offline summary that
//! are **not** a game and could not be represented by an `App` without making
//! `state: GameState` an [`Option`] that fifty-odd call sites would then have to
//! unwrap.
//!
//! Today it holds one thing, and the split is already worth its keep: the redraw
//! policy left `App` with it. *When* to ask the terminal for a frame is a question
//! about the session — which screen is up, whether anything moved — while *what
//! changed* is the run's answer, and [`App::advance`] now returns it rather than
//! writing into a flag it no longer owns.

use std::time::{Duration, Instant};

use color_eyre::Result;
use ratatui::{Terminal, backend::Backend};

use crate::{
    app::App,
    event::{Event, Events},
    keymap,
};

/// The shortest gap between two draws — a **ceiling on the redraw rate**, not a
/// cadence.
///
/// The loop draws when something changed *and* this much time has passed, so a burst
/// of held keys cannot ask the terminal for two hundred frames a second. It is
/// deliberately shorter than the simulation period: today the simulation is the only
/// thing that changes the screen, so the real rate is 20 fps and this ceiling never
/// binds — but the proc flash (two stages of ~100 ms, `docs/UI.md` §7) changes the
/// screen *between* ticks, and it is the reason the two clocks are separate now
/// rather than separated later.
/// **Input is exempt**: a key that meant something draws on the spot, because the
/// only burst it can produce is bounded by the terminal's own repeat rate, and 33 ms
/// of latency in the one place the player is looking is worse than a frame nobody
/// asked for.
const FRAME_PERIOD: Duration = Duration::from_millis(33);

/// A running session, and the loop that drives it.
#[derive(Debug)]
pub struct Session {
    /// The game this session is showing.
    ///
    /// One field today. It becomes one variant of a state enum when the boot routing
    /// lands, which is the whole reason this type exists at a size that looks like a
    /// wrapper: a splash screen has no run, and an `App` whose `state` were optional
    /// would let every screen ask a question only the splash can answer.
    app: App,
    /// The earliest the next draw may happen — [`FRAME_PERIOD`]'s ceiling.
    ///
    /// **A deadline, not a countdown**, the same shape the simulation's clock uses
    /// inside [`App`]: it is compared against the wall clock and reset from it, so a
    /// late pass does not push every later frame back by what it overshot.
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
}

impl Session {
    /// A session showing `app`, with both clocks due.
    ///
    /// Due rather than one period out, so the opening pass draws immediately: a first
    /// frame that waited for the first keypress would leave the player looking at a
    /// blank terminal.
    pub fn new(app: App) -> Self {
        Self {
            app,
            next_frame: Instant::now(),
            dirty: true,
        }
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
    /// [`EventHandler`](crate::event::EventHandler), and between them they made this
    /// function unreachable from a test: the backend writes to the real stdout, and
    /// the handler's thread dies the moment it polls a terminal that is not there.
    /// Everything else in the crate is exercised through ratatui's own `TestBackend`;
    /// these two parameters are what let the loop join it.
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
        while !self.app.should_quit {
            let now = Instant::now();
            // Before the wait, not after: the first frame must show the run as it
            // stands rather than appearing on the first keypress. `new` starts both
            // flags due, so the opening pass always draws.
            if self.dirty && now >= self.next_frame {
                self.app.sync_view(now);
                terminal.draw(|frame| self.app.render(frame, now))?;
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
                    if let Some(action) = keymap::resolve(&self.app, key) {
                        self.app.update(action);
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
            self.dirty |= self.app.advance(Instant::now());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        backend::TestBackend,
        buffer::Buffer,
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    };
    use skylode_core::game::GameState;

    use super::*;

    /// The seed every test session starts from.
    ///
    /// Any value would do; what matters is that it is *fixed*. `GameState::new` draws
    /// the opening mine's whole grid from it, so a seed off the clock would hand each
    /// run of the suite a different picture.
    ///
    /// Spelled here as well as in `app`'s own tests rather than shared: a test fixture
    /// reached across module boundaries would need the test module itself made
    /// `pub(crate)`, which is a larger seam than two constants are worth.
    const SEED: u64 = 0x5B1_0DE;

    /// A session over a fixed run — what every test below opens with.
    ///
    /// `UNIX_EPOCH` as `now` for the seed's reason: it is the offline accrual's
    /// reference point, so a test that read the clock would be measuring how long ago
    /// the file was written.
    fn session() -> Session {
        Session::new(App::new(GameState::new(SEED, std::time::UNIX_EPOCH)))
    }

    /// Every row of the frame, joined — for "is this text on screen anywhere".
    fn whole_frame(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// An event source that reads from a script instead of from a terminal.
    ///
    /// The whole reason [`Session::run`] is generic. It hands out the scripted events
    /// in order and then returns an error, which is how the loop is made to stop even
    /// if the script never quits: a real `EventHandler` blocks forever waiting for a
    /// key that a test will never press, so "the script ran out" has to be a *failure*
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
}
