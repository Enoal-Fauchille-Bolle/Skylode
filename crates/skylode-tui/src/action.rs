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
    /// The mine key says it is down (`Space` on the Mine screen).
    ///
    /// **It announces a key, not a swing**, and the difference is the whole of
    /// `docs/SYSTEMS.md`'s keyboard section. A terminal never reports a *release*, so
    /// "is the player mining" cannot be read off one event: it is
    /// [`App::advance`](crate::app::App::advance)'s answer, from the instant this
    /// gesture last arrived. Which is also why this carries no time — an [`Action`] is
    /// decoded by [`crate::keymap`], which has no clock and must not grow one.
    ///
    /// Auto-repeat produces a stream of these while the key is held, on every
    /// terminal: as a fresh press under the legacy encoding, as
    /// [`KeyEventKind::Repeat`] under the kitty protocol. Both refresh the same
    /// window, which is why one variant covers both.
    ///
    /// [`KeyEventKind::Repeat`]: ratatui::crossterm::event::KeyEventKind::Repeat
    MinePressed,
    /// The mine key says it is up — **only ever emitted by a terminal that reports
    /// releases at all** (the kitty keyboard protocol).
    ///
    /// Not the counterpart of [`MinePressed`](Action::MinePressed) so much as an
    /// *early cut* of the window it opens: where the release is unreportable, the
    /// window expires on its own and mining stops up to `HOLD_WINDOW` later. Nothing
    /// downstream branches on which of the two happened, which is what keeps the
    /// exact and the inferred path one mechanism instead of two.
    MineReleased,
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
    /// Walk from the Upgrades screen to the Inventory, onto the pile a
    /// `compress first` refusal named (`c`).
    ///
    /// **The second half of UI.md §8.4's loop, and it carries no material** — for the
    /// same reason no list gesture carries a row: [`keymap`](crate::keymap) decodes a
    /// key with no access to the run, so it cannot know what was refused. What it names
    /// is the *journey*; [`App`](crate::app::App) supplies the destination from the
    /// refusal it already remembers.
    ///
    /// It travels even when nothing is remembered, landing on the Inventory with the
    /// cursor where the player left it. A key that silently did nothing would be worse
    /// than one that does the obvious half of its job — and the refusal that
    /// advertises it is the only place it is advertised, so an unprompted press is
    /// already the rarer case.
    ///
    /// Bound to `c` on the Upgrades screen, which is free there and is the same
    /// mnemonic [`Compress`](Action::Compress) spends on the Inventory: one letter,
    /// one idea, on the two screens the loop runs between.
    GoCompress,
    /// Collect the level reward the Levels cursor is sitting on (`Enter`).
    ///
    /// Not a variant of its own: `Enter` on this screen *is*
    /// [`Confirm`](Action::Confirm), acting on the row under the cursor exactly as it
    /// does on the other three lists. Named here only so the doc for the screen has
    /// somewhere to point — see [`ClaimAll`](Action::ClaimAll), which does need one.
    ///
    /// Collect **everything** waiting on the ladder (`A`).
    ///
    /// A gesture of its own rather than a [`Confirm`](Action::Confirm) the screen
    /// interprets, for [`BuyMax`](Action::BuyMax)'s reason: acting on one row and
    /// acting on all of them are different acts, and folding them into one key would
    /// leave the player unable to collect a single level deliberately.
    ///
    /// **Not [`AdjustMax`](Action::AdjustMax)**, whose key is also a letter `a`. That
    /// one moves a *number* to its ceiling inside a dialog; this one empties a queue.
    /// The two never coexist on a screen, which is exactly why keeping them apart
    /// matters — a shared variant would be a coincidence of keycaps standing in for a
    /// shared meaning.
    ClaimAll,
    /// Open the dev menu (`` ` ``), on a debug build whose session asked for it.
    ///
    /// **Not [`OpenHelp`](Action::OpenHelp) with a different target**, though both open
    /// an overlay from anywhere: this one is compiled out of a release build, and a
    /// shared variant would leave the release binary carrying a name for something it
    /// cannot do. The `#[cfg]` on a *variant* is what makes the reducer's exhaustive
    /// `match` shrink to match, so nothing downstream needs a runtime guard.
    #[cfg(debug_assertions)]
    OpenDevMenu,
    /// Put the active screen's cursor back where the player actually is (`Home`).
    ///
    /// The Levels roadmap is fifty rows and the cursor opens on the player's own
    /// level, so scrolling away is easy and scrolling back is fifty presses. Generic
    /// in the same way the list gestures are: `keymap` cannot know what "where the
    /// player is" means on a given screen, so the reducer answers it.
    JumpToCurrent,
}
