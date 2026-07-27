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
mod app;
mod config;
mod event;
mod format;
mod keymap;
mod overlay;
mod palette;
mod screen;
mod theme;
mod toast;
mod view;
mod widget;

use color_eyre::Result;

use crate::{app::App, event::EventHandler};

/// How often the event thread emits a heartbeat, in milliseconds.
///
/// This is the *UI* heartbeat — it expires toasts and will later drive redraws.
/// It is not the game's 20 tps simulation tick, which arrives with `tick()` in
/// phase 7 and is a separate clock on purpose: rendering cadence must not be able
/// to change game balance.
const TICK_RATE_MS: u64 = 250;

fn main() -> Result<()> {
    // Pretty panic and error reports. Installed first so that anything failing
    // below is still reported legibly.
    color_eyre::install()?;

    // `init` enables raw mode, switches to the alternate screen, and installs a
    // panic hook that restores both before any message is printed — the reason
    // this crate no longer hand-rolls terminal setup.
    let terminal = ratatui::init();
    let events = EventHandler::new(TICK_RATE_MS);

    // The result is held, not propagated with `?`: the terminal must be restored
    // first, or an error would print into the alternate screen and vanish with it.
    let result = App::new().run(terminal, events);

    ratatui::restore();
    result
}
