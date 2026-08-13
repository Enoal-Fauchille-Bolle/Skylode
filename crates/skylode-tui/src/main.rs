//! Skylode's terminal front-end.
//!
//! The binary is deliberately thin: it installs error reporting, reads the two
//! things only the outside can answer — where a save lives and what time it is —
//! hands the terminal to [`session::Session`], and restores it afterwards.
//! Everything else lives in the modules below, arranged around one boundary — *raw
//! input* becomes a *semantic action* exactly once, in [`keymap`], so that
//! [`app::App::update`] can be exercised without a terminal at all.
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
mod capability;
mod config;
mod cursor;
mod event;
mod flash;
mod format;
mod keymap;
mod overlay;
mod palette;
mod persist;
mod screen;
mod session;
mod theme;
mod toast;
mod view;
mod widget;

use std::{ffi::OsString, io::stdout, time::SystemTime};

use crate::{capability::Capabilities, event::EventHandler, session::Session};
use color_eyre::Result;
use ratatui::crossterm::{
    event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute,
    terminal::supports_keyboard_enhancement,
};

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

/// What this binary calls itself when it speaks to a shell.
///
/// `CARGO_BIN_NAME` and **not** `CARGO_PKG_NAME`, which would print `skylode-tui`.
/// The manifest separates those two names on purpose — the package names a place in
/// the workspace, the binary names the thing a player launches — and a `--version`
/// line that answers with the package name hands them a string they cannot type.
const NAME: &str = env!("CARGO_BIN_NAME");

/// The number this build carries, resolved when it was compiled.
///
/// `env!` is expanded by the compiler, so this is a literal baked into the binary
/// rather than a lookup at start-up: the version printed is the manifest's *by
/// construction*, and no packaging step can make the two disagree. That is what makes
/// the workspace's single inherited `version` worth having.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the command line asked this binary to do.
///
/// Three variants and no error case, deliberately. A TUI has no grammar to defend:
/// someone who types an option this program never had wants to play, and refusing them
/// a session over it trades a working game for a lecture. Only the two flags that
/// *answer something short enough to print* cut the launch short.
#[derive(Debug, PartialEq, Eq)]
enum Request {
    /// Start a session — everything this binary normally does.
    Play,
    /// Print [`VERSION`] and stop.
    Version,
    /// Print the usage summary and stop.
    Usage,
}

/// Reads the command line without going to look for it.
///
/// It takes an iterator instead of calling [`std::env::args_os`] itself, for the same
/// reason [`Session::boot`] takes `now` and a save location: the environment is the
/// outside, and [`main`] is the only place allowed to be outside. A function that read
/// argv directly could not be exercised by a test, and this one is the half of the
/// binary that *can* be — which matters, since `main` itself is unreachable without a
/// real tty.
///
/// `OsString` rather than `String` because a command line is not guaranteed to be
/// valid UTF-8 on any platform. [`std::ffi::OsStr::to_str`] returns [`None`] for the rest, which
/// falls through to [`Request::Play`] — the right answer, and one that costs no
/// decoding rule invented to settle a question that is only ever "is this argument one
/// of four literals?".
fn requested(args: impl Iterator<Item = OsString>) -> Request {
    for arg in args {
        match arg.to_str() {
            Some("--version" | "-V") => return Request::Version,
            Some("--help" | "-h") => return Request::Usage,
            _ => {}
        }
    }
    Request::Play
}

/// The usage summary, kept to what a shell can answer.
///
/// It lists two flags and then points at `?`, rather than reproducing the key bindings:
/// those live in [`keymap`] and change with it, and a copy here would be the second
/// place to update and the first to go stale.
fn usage() -> String {
    format!(
        "{NAME} {VERSION}
A solo terminal idle mining game.

Usage: {NAME} [OPTIONS]

Options:
  -V, --version  Print the version and exit
  -h, --help     Print this message and exit

Everything else is keyboard-driven from inside the game: press `?` on any
screen for the full list of bindings."
    )
}

fn main() -> Result<()> {
    // Answered before anything else is installed, entered or allocated.
    //
    // Not merely an optimisation: `ratatui::init` switches to the alternate screen,
    // and anything printed after it is wiped when the terminal is restored. A
    // `--version` handled further down would write its line into a buffer nobody ever
    // sees. This is the same hazard `main` already guards against by holding the
    // session's result instead of propagating it with `?`.
    match requested(std::env::args_os().skip(1)) {
        Request::Version => {
            println!("{NAME} {VERSION}");
            return Ok(());
        }
        Request::Usage => {
            println!("{}", usage());
            return Ok(());
        }
        Request::Play => {}
    }

    // Pretty panic and error reports. Installed first so that anything failing
    // below is still reported legibly.
    color_eyre::install()?;

    // The readings this binary owes the rest of the program: where the platform keeps
    // a save, what time it is, and what the terminal says about its palette. All three
    // are the environment, and `main` is the outside — everything below takes them as
    // arguments so that a test can choose them.
    let slots = persist::location();
    let session = Session::boot(slots, SystemTime::now()).with_capabilities(Capabilities::detect());

    // `init` enables raw mode, switches to the alternate screen, and installs a
    // panic hook that restores both before any message is printed — the reason
    // this crate no longer hand-rolls terminal setup.
    let mut terminal = ratatui::init();
    let enhanced = enable_key_releases();
    let events = EventHandler::new(TICK_RATE_MS);

    // The dev menu's *activation*, and the only place the environment is read for it.
    // The compilation gate is `#[cfg(debug_assertions)]`, applied at every door down to
    // `skylode_core::game::dev`; this line is the second layer, so that an ordinary
    // `cargo run` is an ordinary game. In a release build the whole statement is absent
    // along with the method it calls.
    #[cfg(debug_assertions)]
    let session = session.with_dev(dev_requested());

    // The result is held, not propagated with `?`: the terminal must be restored
    // first, or an error would print into the alternate screen and vanish with it.
    let result = session.run(&mut terminal, events);

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
/// It reads the environment, which is legal here for [`persist::location`]'s reason and
/// no other: `main` is the outside. The reading is spent immediately on
/// [`Session::with_dev`] and never consulted again, so nothing below this function can
/// ask the environment what mode it is in.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape a caller hands [`requested`], spelled once.
    fn args(raw: &[&str]) -> impl Iterator<Item = OsString> {
        raw.iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
            .into_iter()
    }

    #[test]
    fn an_empty_command_line_starts_the_game() {
        assert_eq!(requested(args(&[])), Request::Play);
    }

    #[test]
    fn both_spellings_of_the_version_flag_ask_for_the_version() {
        assert_eq!(requested(args(&["--version"])), Request::Version);
        assert_eq!(requested(args(&["-V"])), Request::Version);
    }

    #[test]
    fn both_spellings_of_the_help_flag_ask_for_usage() {
        assert_eq!(requested(args(&["--help"])), Request::Usage);
        assert_eq!(requested(args(&["-h"])), Request::Usage);
    }

    /// A lowercase `-v` is *not* the version flag.
    ///
    /// The two are one keystroke apart and the convention is near-universal: `-V` is
    /// the version, `-v` is verbosity. Answering the version to `-v` would make this
    /// binary the odd one out for a player who types it out of habit from every other
    /// tool, so it falls through and starts the game.
    #[test]
    fn a_lowercase_v_is_not_the_version_flag() {
        assert_eq!(requested(args(&["-v"])), Request::Play);
    }

    #[test]
    fn an_option_this_program_never_had_starts_the_game() {
        assert_eq!(requested(args(&["--fullscreen"])), Request::Play);
    }

    /// Whichever flag is met first wins, rather than one outranking the other.
    ///
    /// Both answers are short and correct, so there is no reading of `--help --version`
    /// worth defending over the other; picking the first keeps the rule to one sentence
    /// and spares a precedence table nobody would ever look up.
    #[test]
    fn the_first_flag_met_is_the_one_answered() {
        assert_eq!(requested(args(&["--help", "--version"])), Request::Usage);
        assert_eq!(requested(args(&["--version", "--help"])), Request::Version);
    }

    /// A flag still counts when something unrecognised came before it.
    #[test]
    fn a_flag_is_found_past_an_argument_that_means_nothing() {
        assert_eq!(
            requested(args(&["nonsense", "--version"])),
            Request::Version
        );
    }

    /// An argument that is not valid UTF-8 is not a flag, and does not stop the search.
    ///
    /// Unix only because it is the only platform where such an argument can be built
    /// from bytes: Windows command lines are UTF-16, and its `OsString` has no
    /// equivalent constructor taking arbitrary bytes. The behaviour under test is
    /// [`std::ffi::OsStr::to_str`] returning [`None`], which is the same on both.
    #[cfg(unix)]
    #[test]
    fn an_argument_that_is_not_utf8_is_simply_not_a_flag() {
        use std::os::unix::ffi::OsStringExt;

        let invalid = OsString::from_vec(vec![0xff, 0xfe]);
        let line = vec![invalid, OsString::from("--version")];
        assert_eq!(requested(line.into_iter()), Request::Version);
    }

    /// The usage text names both flags and sends the reader to `?` for the rest.
    ///
    /// Asserting on the content rather than the exact string: the wording is meant to
    /// be edited freely, and a test pinning it whole would refuse every improvement
    /// while catching nothing that matters.
    #[test]
    fn the_usage_text_lists_both_flags_and_defers_to_the_game_for_keys() {
        let text = usage();
        assert!(text.contains("--version"));
        assert!(text.contains("--help"));
        assert!(text.contains('?'));
        assert!(text.starts_with(NAME));
    }

    /// The binary calls itself `skylode`, not `skylode-tui`.
    ///
    /// This is the whole point of [`NAME`] reading `CARGO_BIN_NAME`, and it is worth an
    /// assertion because the two constants differ by one word and swapping them would
    /// still compile, still print, and still look right in review.
    #[test]
    fn the_binary_answers_with_the_name_a_player_types() {
        assert_eq!(NAME, "skylode");
    }
}
