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
///
/// **The list gestures name a movement, not a screen** — [`Action::CursorUp`], not
/// a `MinesCursorUp` — and that is forced rather than chosen. [`crate::keymap`]
/// decodes a key with no access to the run, so it *cannot* produce a
/// `SelectMine(kind)`: it does not know where the cursor is. Which screen a gesture
/// lands on is therefore resolved in `update`, where the state is. The payoff is
/// that the Inventory table, the Upgrades ladders and the Levels roadmap all reuse
/// these five instead of adding five apiece.
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
    /// Move the active screen's list selection up one row.
    CursorUp,
    /// Move it down one row.
    CursorDown,
    /// Turn the active screen's value down one step — the richness dial, or the
    /// compression spinner while its dialog is up.
    AdjustLeft,
    /// Turn it up one step.
    AdjustRight,
    /// Push that value straight to its maximum (`a`) — the compression dialog's
    /// *all*.
    ///
    /// A gesture and not a purchase: it names *"set this to the most it can be"*,
    /// which is why the Upgrades screen's `M` (buy to the cap) will not reuse it. One
    /// moves a number, the other spends an inventory.
    AdjustMax,
    /// Act on whatever the cursor is sitting on (`Enter`).
    Confirm,
    /// Buy as much of the active track as the inventory allows (`M`).
    ///
    /// **Not [`AdjustMax`](Action::AdjustMax), and the two are worth keeping apart.**
    /// That one moves a *number* to its ceiling — the compression spinner's `a` — and
    /// costs nothing; this one spends an inventory and cannot be taken back. One
    /// gesture, one kind of consequence.
    BuyMax,
    /// Show the next Upgrades sub-tab (the configurable binding, `⇧→` by default).
    ///
    /// A gesture of its own rather than a reuse of
    /// [`AdjustRight`](Action::AdjustRight), because UI.md §9 gives the two axes
    /// different jobs everywhere: `←/→` adjusts the value under the cursor, and the
    /// lateral *sub-tab* move is a different act that happens to be bound near it.
    NextSubTab,
    /// Show the previous one.
    PrevSubTab,
    /// Open the compression dialog on the material under the Inventory cursor (`c`).
    ///
    /// **Not a list gesture**, which is why it is a variant of its own rather than a
    /// [`Confirm`](Action::Confirm) the Inventory screen interprets: opening a modal
    /// is a different kind of act from acting on a row, and the two keys that do it
    /// are `c` and `C` rather than `Enter`.
    Compress,
    /// Open the same dialog with the arithmetic reversed (`C`).
    Decompress,
}
