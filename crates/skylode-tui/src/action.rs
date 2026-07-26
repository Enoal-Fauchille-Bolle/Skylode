//! The semantic input vocabulary.
//!
//! An [`Action`] is *what the player wants to happen*, decoded from a raw
//! [`crate::event::Event`] by [`crate::keymap`]. Splitting the two lets
//! [`crate::app::App::update`] be a pure function of `(state, Action)` — no
//! `KeyEvent`, no crossterm, no terminal — so the whole app logic is unit-testable.
//! It is the same discipline the core follows: rules that can be exercised without
//! a screen.

/// A decoded player intent.
///
/// New screens and modals add variants here; the exhaustive `match` in
/// [`crate::app::App::update`] then refuses to compile until each one is handled,
/// so the compiler keeps the reducer complete.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Leave the game (also the target of `Ctrl-C`). For now it exits the
    /// process; once the session state machine (UI-EN.md §6.3) lands it will
    /// return to the splash instead.
    Quit,
    /// Advance one tab along the ring, wrapping past the last back to the first.
    NextScreen,
    /// Step back one tab, wrapping the other way.
    PrevScreen,
    /// Jump straight to the tab at this zero-based index (the `1`..`6` keys).
    SelectScreen(usize),
    /// Raise an ephemeral toast. A stand-in until the tick returns real events
    /// (UI-EN.md §6.2); wired to a demo key so the overlay path can be exercised.
    ShowToast(String),
    /// Open the Help overlay (`?` from any screen). It stacks over the current
    /// screen, which is what Help then reports the bindings of.
    OpenHelp,
    /// Dismiss the stacked modal (`Esc`, or `?` while Help is up). Names the act,
    /// not the modal, so a second modal reuses it rather than adding a close per
    /// variant.
    CloseModal,
}
