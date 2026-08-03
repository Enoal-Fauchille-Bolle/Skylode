//! Skylode's terminal front-end.
//!
//! The binary is deliberately thin: it installs error reporting, hands the
//! terminal to [`app::App`], and restores it afterwards. Everything else lives in
//! the modules below, arranged around one boundary — *raw input* becomes a
//! *semantic action* exactly once, in [`keymap`], so that [`app::App::update`]
//! can be exercised without a terminal at all.
//!
//! The design this implements is `organization/UI-EN.md`; the game rules it will
//! eventually render live in `skylode-core`, which this crate may read but never
//! duplicate.

// Each module documents itself with `//!` in its own file. Adding a `///` here
// as well would merge the two, and rustdoc would then resolve the module's links
// from *this* scope — where none of its items exist.
mod action;
mod announce;
mod app;
mod config;
mod cursor;
mod event;
mod flash;
mod format;
mod keymap;
mod overlay;
mod palette;
mod screen;
mod theme;
mod toast;
mod view;
mod widget;

use std::{
    io::stdout,
    time::{SystemTime, UNIX_EPOCH},
};

use color_eyre::Result;
use ratatui::crossterm::{
    event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute,
    terminal::supports_keyboard_enhancement,
};
use skylode_core::game::GameState;

use crate::{app::App, event::EventHandler};

/// How often the event thread wakes the render loop, in milliseconds.
///
/// **A sampling period, not a cadence**, and the difference is the whole reason it
/// dropped from 250 to 10. The two rates that matter — the game's 20 tps simulation
/// and the ~30 fps redraw ceiling — are deadlines held in `app::App` and compared
/// against the wall clock, so neither counts heartbeats. What this number decides is
/// only the *resolution* at which the loop notices a deadline has passed: at 10 ms,
/// a 50 ms step never slips by more than a fifth of itself.
///
/// It costs a hundred channel receives a second and no game state at all. Making it
/// the simulation rate instead would tie the two together again, and a rendering
/// cadence must never be able to change game balance.
const TICK_RATE_MS: u64 = 10;

/// The seed a fresh run starts from, taken from the clock.
///
/// **This is the only entropy in the game, and it is deliberately on this side of
/// the crate boundary.** `skylode-core` compiles `rand` with
/// `default-features = false`, which strips `thread_rng` and `os_rng` out of the
/// build entirely — so the determinism contract is enforced by the compiler rather
/// than by discipline, and a seed *has* to be handed in from outside. Here is
/// outside.
///
/// Nanoseconds since the epoch, not seconds: two runs started in the same second
/// should not lay out the same mine. A clock before 1970 falls back to `0`, which
/// is a legal seed — a wrong clock should give a boring run, not no run.
///
/// Phase 7 takes this over: a loaded save carries its own generator, position
/// included, and only a genuinely new run reaches this line.
fn seed_from_clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since_epoch| since_epoch.as_nanos() as u64)
}

fn main() -> Result<()> {
    // Pretty panic and error reports. Installed first so that anything failing
    // below is still reported legibly.
    color_eyre::install()?;

    // A fresh run. `now` is passed in rather than read inside the core, which is the
    // same rule as the seed: the rules take the clock as an argument so that a test
    // can choose it.
    let state = GameState::new(seed_from_clock(), SystemTime::now());

    // `init` enables raw mode, switches to the alternate screen, and installs a
    // panic hook that restores both before any message is printed — the reason
    // this crate no longer hand-rolls terminal setup.
    let mut terminal = ratatui::init();
    let enhanced = enable_key_releases();
    let events = EventHandler::new(TICK_RATE_MS);

    let app = App::new(state);
    // The dev menu's *activation*, and the only place the environment is read for it.
    // The compilation gate is `#[cfg(debug_assertions)]`, applied at every door down to
    // `skylode_core::game::dev`; this line is the second layer, so that an ordinary
    // `cargo run` is an ordinary game. In a release build the whole statement is absent
    // along with the method it calls.
    #[cfg(debug_assertions)]
    let app = app.with_dev(dev_requested());

    // The result is held, not propagated with `?`: the terminal must be restored
    // first, or an error would print into the alternate screen and vanish with it.
    let result = app.run(&mut terminal, events);

    if enhanced {
        // Before `restore`, and unconditional on how the loop ended: these flags are
        // *terminal* state, not process state, so leaking them leaves the shell — and
        // every program the player runs next — in a mode it did not ask for.
        let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
    }
    ratatui::restore();
    result
}

/// Whether this session was started with the dev menu asked for.
///
/// **Presence, not a value**, so `SKYLODE_DEV=0` enables it too. A variable whose only
/// job is to be set has no business having a grammar of truthy strings — and the one
/// person who types it is the one who wrote this line.
///
/// It reads the environment, which is legal here for [`seed_from_clock`]'s reason and no
/// other: `main` is the outside. The reading is spent immediately on
/// [`App::with_dev`](crate::app::App::with_dev) and never consulted again, so nothing
/// below this function can ask the environment what mode it is in.
///
/// `#[cfg(debug_assertions)]` because there is nothing for it to enable in a release
/// build: `App` has no `dev` field there, and `keymap` has no branch that would read it.
#[cfg(debug_assertions)]
fn dev_requested() -> bool {
    std::env::var_os("SKYLODE_DEV").is_some()
}

/// Asks the terminal to report key releases, and says whether it agreed.
///
/// **The exact half of `docs/SYSTEMS.md`'s two-layer input scheme.** A terminal
/// speaking the legacy encoding never reports a release — a key *was* a character
/// there, and a character has no duration — so `app` infers the hold from a window
/// instead. Where this succeeds, a real release arrives and cuts that window early;
/// where it does not, nothing downstream changes and the window answers alone. That
/// is why the return value goes no further than [`main`]: it decides what to pop on
/// the way out, and nothing about how the game reads input.
///
/// **Both flags, and the second is the load-bearing one.** `REPORT_EVENT_TYPES` alone
/// is silently useless for this: the protocol still sends text-producing keys as raw
/// UTF-8, and `Space` produces text, so it would arrive as a bare `0x20` with no
/// event type attached. `REPORT_ALL_KEYS_AS_ESCAPE_CODES` is what forces it down the
/// path where a release can exist at all.
///
/// The query round-trips to the terminal, so it runs **before** the event thread
/// starts: the reply arrives on the same input stream the poller is about to drain,
/// and a thread already reading would swallow it.
///
/// Failure of any kind means "no releases here", which is exactly the fallback the
/// window already implements — hence `unwrap_or(false)` and a discarded `execute!`
/// result rather than a `?` that would end a session over a decoration.
fn enable_key_releases() -> bool {
    if !supports_keyboard_enhancement().unwrap_or(false) {
        return false;
    }
    execute!(
        stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    )
    .is_ok()
}
