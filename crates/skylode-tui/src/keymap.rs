//! Where a keystroke becomes an intent.
//!
//! This is the one place that knows about `KeyCode`. Everything downstream speaks
//! [`Action`], which is what keeps [`crate::app::App::update`] testable without a
//! terminal. It is also where UI-EN.md §9's configurable sub-tab binding will
//! land: one function to change, not a `match` scattered across six screens.
//!
//! **Three functions and not one**, because the game is no longer the only thing on
//! screen. [`resolve`] answers a run; [`resolve_menu`] answers the states that are
//! not a run at all (the title, the recovery frames, and the Settings screen the
//! title opens) in [`MenuAction`]'s six gestures; and [`resolve_too_small`] answers
//! the one screen that is drawn over
//! every other. They live together because the invariant above is about the *module*:
//! a `KeyCode` named anywhere else is a binding nobody can find.
//!
//! **Resolution order**, and it matters:
//! 0. A key *release* — it can only ever mean "stop mining", and nothing else here
//!    may see it.
//! 1. `Ctrl-C` — always quits, even if a modal would otherwise capture the key.
//! 2. An open modal captures everything else (it is modal; that is the point).
//! 3. The dev menu's key, where the build and the session both allow one.
//! 4. The global ring bindings.
//! 5. The configurable sub-tab binding.
//! 6. The active screen's contextual bindings.
//!
//! Globals are consulted *before* the screen so that `Tab` cannot be shadowed by
//! a screen that forgot the ring exists.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::{
    action::{Action, MenuAction},
    app::App,
    config::SubTabKeys,
    overlay::Modal,
    screen::Screen,
};

/// The digits that jump straight to a tab. Derived from the ring, so a seventh
/// screen does not need this constant edited — only the ring.
const FIRST_TAB_DIGIT: char = '1';

/// Translates a key press on a menu screen, or `None` if nothing is bound there.
///
/// **One resolver for the title and for the recovery frames**, because they are the
/// same interaction: a short list, a caret, `Enter`. Two functions would have to be
/// kept identical by hand, and the moment they drifted the recovery screen would be
/// the one that lost a key — it is the screen nobody tests by playing.
///
/// It takes no state at all, unlike [`resolve`]. Nothing here is contextual: `Enter`
/// means *take the row the caret is on* whatever that row happens to be, and which
/// row that is belongs to the caller that owns the cursor.
///
/// **`Ctrl-C` is here too, and it is the same answer as `q`.** On a screen with no
/// game behind it there is no third thing quitting could mean.
pub fn resolve_menu(key: KeyEvent) -> Option<MenuAction> {
    // A release is never a menu gesture. It reaches this far only on a terminal
    // speaking the kitty protocol, where every key is reported twice — so without
    // this the caret would move two rows per press there and one row everywhere else.
    if key.kind == KeyEventKind::Release {
        return None;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'C'))
    {
        return Some(MenuAction::Quit);
    }
    match key.code {
        // Arrows only, matching the game's own lists: no screen in Skylode binds
        // `j`/`k`, and a menu that did would be teaching a gesture that works nowhere
        // else.
        KeyCode::Up => Some(MenuAction::Up),
        KeyCode::Down => Some(MenuAction::Down),
        // The lateral pair means what it means everywhere else — *adjust whatever the
        // cursor is on* (`docs/UI.md` §9). It is bound unconditionally rather than only
        // while Settings is up, because this function deliberately takes no state: which
        // list is showing is the caller's question, and a menu with nothing to adjust
        // simply drops the gesture where it lands.
        KeyCode::Left => Some(MenuAction::Left),
        KeyCode::Right => Some(MenuAction::Right),
        KeyCode::Enter => Some(MenuAction::Confirm),
        // Declining, wherever something was asked. It is `Esc` here because it is `Esc`
        // in every modal the game already has, and a box that asked to be declined with
        // a different key would be teaching a second habit for one question.
        KeyCode::Esc => Some(MenuAction::Cancel),
        KeyCode::Char('q') => Some(MenuAction::Quit),
        _ => None,
    }
}

/// Translates a key press while the terminal is too small to draw anything.
///
/// **The narrowest resolver in the crate, and deliberately so.** §6.2's screen prints
/// exactly one affordance — *"Enlarge the window, or press q to quit"* — and it is
/// drawn over every state, including the title. So `q` here has to mean the process
/// and not "back to the title": the title is a screen this terminal cannot draw
/// either, and a key that promised to quit and did not would be the one lie on a
/// frame whose whole job is to be readable when nothing else is.
pub fn resolve_too_small(key: KeyEvent) -> Option<MenuAction> {
    if key.kind == KeyEventKind::Release {
        return None;
    }
    let control_c = key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('c' | 'C'));
    (control_c || key.code == KeyCode::Char('q')).then_some(MenuAction::Quit)
}

/// Translates a key press into an [`Action`], or `None` if nothing is bound.
///
/// Takes `&App` rather than just the screen because resolution is *contextual*:
/// the same key means different things depending on whether a modal is open.
pub fn resolve(app: &App, key: KeyEvent) -> Option<Action> {
    // 0. A release, before anything else can mistake it for a press.
    //
    //    `event` stopped filtering key kinds so that the mine key's release could
    //    reach us at all, and this branch is the whole price of that: without it,
    //    every binding below would fire twice on a terminal that reports releases —
    //    once on the way down and once on the way up — and `Tab` would advance two
    //    tabs per keystroke on kitty and one everywhere else.
    //
    //    So exactly one release is meaningful, and it is answered here rather than
    //    in `Screen::map_key`: that function is handed a key and nothing else, so a
    //    release reaching it would arrive indistinguishable from a press.
    //
    //    It is answered *above* the modal branch too, which is the one place a
    //    release outranks "a modal captures every key". A modal cannot use a release,
    //    and swallowing this one would leave the pickaxe swinging behind the box for
    //    as long as the hold window lasts.
    if key.kind == KeyEventKind::Release {
        return (key.code == KeyCode::Char(' ') && app.screen == Screen::Mine)
            .then_some(Action::MineReleased);
    }

    // 1. Ctrl-C outranks everything, including a modal.
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c' | 'C'))
    {
        return Some(Action::Quit);
    }

    // 2. A modal captures the rest: while one is up, it gets every key the globals
    //    would otherwise claim, which is what "modal" means. Each swallows what it
    //    does not use, so nothing leaks to the screen behind it.
    //
    //    The `match` is exhaustive on purpose: a modal added to the enum cannot be
    //    stacked until someone decides what its keys are.
    //    Borrowed rather than copied since the prestige confirm carries a `String`;
    //    nothing here reads it, so a shared borrow is all the branch ever needed.
    if let Some(modal) = &app.modal {
        return match modal {
            // Help closes on its own key (`?`) or `Esc`.
            Modal::Help => match key.code {
                KeyCode::Char('?') | KeyCode::Esc => Some(Action::CloseModal),
                _ => None,
            },
            // Settings is a list whose rows hold values, so it reuses the four list
            // gestures and names no key of its own — the dev menu's arm exactly, which
            // is the same shape of screen. `s` closes it as well as opening it, like
            // Help's `?`: a key that leads nowhere else should be a toggle.
            Modal::Settings { .. } => match key.code {
                KeyCode::Up => Some(Action::CursorUp),
                KeyCode::Down => Some(Action::CursorDown),
                KeyCode::Left => Some(Action::AdjustLeft),
                KeyCode::Right => Some(Action::AdjustRight),
                KeyCode::Char('s') | KeyCode::Esc => Some(Action::CloseModal),
                _ => None,
            },
            // The compression spinner. It reuses the list gestures rather than
            // naming keys of its own — `←/→` is *adjust the value under the cursor*
            // everywhere in UI.md §9, and a spinner is exactly that — which is why
            // this arm adds one variant (`a`) and not five.
            Modal::Compress { .. } => match key.code {
                KeyCode::Left => Some(Action::AdjustLeft),
                KeyCode::Right => Some(Action::AdjustRight),
                KeyCode::Char('a') => Some(Action::AdjustMax),
                KeyCode::Enter => Some(Action::Confirm),
                KeyCode::Esc => Some(Action::CloseModal),
                _ => None,
            },
            // The tier-jump confirmation. Two options rather than a value, so `←/→`
            // move the caret between them — the same *adjust what is under the
            // cursor* the spinner reuses — and `Enter` takes the focused one. `n` is
            // the frame's own printed key and closes the box outright, which is why
            // it is `CloseModal` and not a third gesture: declining is exactly what
            // every other modal's `Esc` does.
            Modal::Dip { .. } => match key.code {
                KeyCode::Left => Some(Action::AdjustLeft),
                KeyCode::Right => Some(Action::AdjustRight),
                KeyCode::Enter => Some(Action::Confirm),
                KeyCode::Char('n') | KeyCode::Esc => Some(Action::CloseModal),
                _ => None,
            },
            // The prestige preview. Two keys, because it is a *preview*: `Enter` asks
            // to go on, `Esc` closes. It carries no value and no caret, so it borrows
            // no list gesture either — the only modal in the game that is pure reading.
            Modal::PrestigePreview => match key.code {
                KeyCode::Enter => Some(Action::Confirm),
                // §8.4's walk, claimed inside the modal because the modal is what would
                // otherwise swallow it. The refusal it answers is raised *by* this box —
                // the price is quoted in two denominations, so a player holding the value
                // in raw is refused here as surely as on the Upgrades screen — and a `c`
                // that reached the screen behind would be a key the player pressed on the
                // sentence advertising it and that did nothing.
                KeyCode::Char('c') => Some(Action::GoCompress),
                KeyCode::Esc => Some(Action::CloseModal),
                _ => None,
            },
            // The typed confirm, and the one arm in this file that claims *letters*.
            //
            // That is the point rather than an accident: §6.9 asks for eight characters
            // precisely because no other affordance in the keymap can be produced by
            // muscle memory aimed elsewhere. So `q` typed into the field is a `Q` and
            // not a quit, and `1` is a digit and not a tab — the modal capture is what
            // makes that safe, and `Ctrl-C` still quits because rule 1 outranks it.
            Modal::PrestigeConfirm { .. } => match key.code {
                KeyCode::Char(typed) => Some(Action::TypeChar(typed)),
                KeyCode::Backspace => Some(Action::EraseChar),
                KeyCode::Enter => Some(Action::Confirm),
                KeyCode::Esc => Some(Action::CloseModal),
                _ => None,
            },
            // The dev menu: a list, so it reuses the list gestures and names no key of
            // its own. `` ` `` closes it as well as opening it, like Help's `?` — a
            // toggle is what a key that leads *nowhere else* should be.
            #[cfg(debug_assertions)]
            Modal::Dev => match key.code {
                KeyCode::Up => Some(Action::CursorUp),
                KeyCode::Down => Some(Action::CursorDown),
                KeyCode::Left => Some(Action::AdjustLeft),
                KeyCode::Right => Some(Action::AdjustRight),
                KeyCode::Enter => Some(Action::Confirm),
                KeyCode::Char('`') | KeyCode::Esc => Some(Action::CloseModal),
                _ => None,
            },
        };
    }

    // 3. The dev menu, before the ring, and only where it exists.
    //
    //    Two conditions and both are gates: the `#[cfg]` means a release build does not
    //    contain this branch, and `app.dev` means a debug build without `SKYLODE_DEV`
    //    leaves the key unbound. Backquote is free on every screen and is nobody's
    //    mnemonic, which is what a key that must never be pressed by accident wants.
    //
    //    Above the ring rather than below it for the same reason `?` is: it opens from
    //    anywhere, and a screen that claimed `` ` `` would shadow it.
    #[cfg(debug_assertions)]
    if app.dev.is_some() && key.code == KeyCode::Char('`') {
        return Some(Action::OpenDevMenu);
    }

    // 4. The global ring bindings.
    match key.code {
        // **Back to the title, not out of the program** (`docs/UI.md` §8.3). `Ctrl-C`
        // above is the one that ends the process; this one puts the run down, and the
        // session writes it before rebuilding the title from the file.
        KeyCode::Char('q') => return Some(Action::ToTitle),
        // `?` is global and printed in every footer, so it opens Help from anywhere.
        KeyCode::Char('?') => return Some(Action::OpenHelp),
        // `s` is the other global that no footer prints (`docs/UI.md` §9): Help is its
        // only discoverability surface, exactly like `q`. It was documented as a
        // binding for eight phases while being bound nowhere — the one advertised key
        // that did nothing — and this line is the whole of the fix.
        KeyCode::Char('s') => return Some(Action::OpenSettings),
        // **Back to the Mine screen** (`docs/UI.md` §8.1), and deliberately *not*
        // guarded on which screen we are on: `Esc` reads the same sentence on all six
        // tabs, and on Mine it lands where the player already is. A guard would make it
        // an unbound key on one screen out of six, which is a rule with an exception
        // where a rule would do.
        //
        // Here rather than in a screen's `map_key` for `Tab`'s reason: a screen that
        // later claimed `Esc` would shadow it, and the whole value of the binding is
        // that it never has to be looked up. The modal capture above is what keeps the
        // two meanings of the key — *close this box*, *leave this tab* — one gesture
        // resolved in layers instead of two bindings fighting.
        KeyCode::Esc => return Some(Action::ToMine),
        KeyCode::Tab => return Some(Action::NextScreen),
        KeyCode::BackTab => return Some(Action::PrevScreen),
        KeyCode::Char(digit @ '1'..='6') => {
            let index = digit as usize - FIRST_TAB_DIGIT as usize;
            return Some(Action::SelectScreen(index));
        }
        _ => {}
    }

    // 5. The configurable sub-tab binding, before the screen is consulted.
    //
    //    Here rather than in `screen::upgrades::map_key` because that function is
    //    handed a key and nothing else — no config — and this module's header has
    //    promised since phase 0 that the binding would land in "one function, not a
    //    match scattered across six screens". Gated on the Upgrades screen because
    //    two of the three choices (`h`/`l`, `[`/`]`) are ordinary characters: claimed
    //    globally they would be swallowed everywhere for a screen that owns them
    //    nowhere else.
    if app.screen == Screen::Upgrades
        && let Some(action) = sub_tab(key, app.config.sub_tab_keys)
    {
        return Some(action);
    }

    // 6. Fall through to whatever the current screen wants.
    app.screen.map_key(key)
}

/// Decodes the configured sub-tab binding, or `None` if this key is not it.
///
/// The three choices are `SubTabKeys`' own, and each is matched *whole* — a
/// `Shift+←` is the modifier and the code together, and dropping the modifier check
/// would make a bare `←` switch sub-tabs on a screen where UI.md §9 deliberately
/// leaves the lateral axis free.
fn sub_tab(key: KeyEvent, binding: SubTabKeys) -> Option<Action> {
    let shifted = key.modifiers.contains(KeyModifiers::SHIFT);
    match (binding, key.code) {
        (SubTabKeys::ShiftArrows, KeyCode::Left) if shifted => Some(Action::PrevSubTab),
        (SubTabKeys::ShiftArrows, KeyCode::Right) if shifted => Some(Action::NextSubTab),
        (SubTabKeys::HL, KeyCode::Char('h')) => Some(Action::PrevSubTab),
        (SubTabKeys::HL, KeyCode::Char('l')) => Some(Action::NextSubTab),
        (SubTabKeys::Brackets, KeyCode::Char('[')) => Some(Action::PrevSubTab),
        (SubTabKeys::Brackets, KeyCode::Char(']')) => Some(Action::NextSubTab),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use skylode_core::{game::GameState, material::Material};

    use super::*;
    use crate::{overlay::Conversion, screen::Screen};

    /// A plain key press, with no modifiers.
    pub(super) fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// A session to resolve keys against.
    ///
    /// `resolve` reads only which screen is open and whether a modal is stacked, so
    /// the run behind it is immaterial — but it has to be *some* run, and a fixed
    /// seed keeps it from being a different one each time the suite is run.
    pub(super) fn session() -> App {
        App::new(GameState::new(0x5B1_0DE, std::time::UNIX_EPOCH))
    }

    #[test]
    fn tab_advances_the_ring_and_backtab_reverses_it() {
        let app = session();
        assert_eq!(resolve(&app, press(KeyCode::Tab)), Some(Action::NextScreen));
        assert_eq!(
            resolve(&app, press(KeyCode::BackTab)),
            Some(Action::PrevScreen)
        );
    }

    #[test]
    fn the_digit_keys_are_zero_based_tab_indices() {
        let app = session();
        assert_eq!(
            resolve(&app, press(KeyCode::Char('1'))),
            Some(Action::SelectScreen(0))
        );
        assert_eq!(
            resolve(&app, press(KeyCode::Char('6'))),
            Some(Action::SelectScreen(5))
        );
    }

    #[test]
    fn every_tab_digit_resolves_to_a_real_screen() {
        let app = session();
        for (position, _) in Screen::ALL.iter().enumerate() {
            // Built by arithmetic rather than `from_digit` so the test needs no
            // `unwrap`: the ring is six long, so this never leaves '1'..='6'.
            let digit = (b'1' + position as u8) as char;
            let action = resolve(&app, press(KeyCode::Char(digit)));
            assert_eq!(action, Some(Action::SelectScreen(position)));
        }
    }

    #[test]
    fn a_seventh_digit_is_not_bound() {
        let app = session();
        assert_eq!(resolve(&app, press(KeyCode::Char('7'))), None);
    }

    #[test]
    fn q_leaves_for_the_title_and_ctrl_c_leaves_the_program() {
        // The two exits are different actions and not one with a modifier: §8.3 has an
        // edge from a game back to the title and none from a game to the process, and
        // `Ctrl-C` is a terminal convention rather than something the game drew.
        let app = session();
        assert_eq!(
            resolve(&app, press(KeyCode::Char('q'))),
            Some(Action::ToTitle)
        );
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(resolve(&app, ctrl_c), Some(Action::Quit));
    }

    /// **Every tab, Mine included**, and the last one is the point rather than an
    /// oversight: the binding is not gated on the screen, so `Esc` decodes to the same
    /// intent on all six and the Mine screen answers it by already being there. A
    /// resolver that returned `None` there would leave one screen where the key is
    /// unbound, and an unbound key is one a screen can later claim.
    #[test]
    fn esc_returns_to_the_mine_screen_from_every_tab() {
        for screen in Screen::ALL {
            let mut app = session();
            app.screen = screen;
            assert_eq!(
                resolve(&app, press(KeyCode::Esc)),
                Some(Action::ToMine),
                "esc is shadowed on {screen:?}"
            );
        }
    }

    /// The layering, asserted at the seam that produces it.
    ///
    /// `Esc` means *close this box* and *leave this tab*, and nothing decides between
    /// them: the modal branch is simply above the globals, so the first press never
    /// reaches the second meaning. Were the order reversed, a player closing the
    /// prestige preview would find themselves on the Mine screen with the box still up.
    #[test]
    fn esc_closes_a_modal_before_it_leaves_the_screen() {
        let mut app = session();
        app.screen = Screen::Upgrades;
        app.modal = Some(Modal::Help);
        assert_eq!(resolve(&app, press(KeyCode::Esc)), Some(Action::CloseModal));

        // And with nothing stacked, the same key means the other half.
        app.modal = None;
        assert_eq!(resolve(&app, press(KeyCode::Esc)), Some(Action::ToMine));
    }

    #[test]
    fn an_unbound_key_is_declined_rather_than_swallowed() {
        let app = session();
        assert_eq!(resolve(&app, press(KeyCode::Char('z'))), None);
    }

    #[test]
    fn question_mark_opens_help_from_a_screen() {
        let app = session();
        assert_eq!(
            resolve(&app, press(KeyCode::Char('?'))),
            Some(Action::OpenHelp)
        );
    }

    /// **The one global that advertised a key and bound nothing**, until now.
    ///
    /// Asserted on every screen rather than on one, like `Esc` above and for the same
    /// reason: `s` is global, so a screen that later claimed the letter would shadow it
    /// on exactly one tab out of six — the failure nobody notices by playing.
    #[test]
    fn s_opens_settings_from_every_tab() {
        for screen in Screen::ALL {
            let mut app = session();
            app.screen = screen;
            assert_eq!(
                resolve(&app, press(KeyCode::Char('s'))),
                Some(Action::OpenSettings),
                "s is shadowed on {screen:?}"
            );
        }
    }

    /// Settings captures the keys and closes on its own letter or on `Esc` — Help's
    /// contract exactly, which is what makes the two overlays one habit.
    #[test]
    fn while_settings_is_up_it_captures_the_keys_and_closes_on_s_or_esc() {
        let mut app = session();
        app.modal = Some(Modal::Settings {
            row: crate::overlay::settings::ROWS[0],
        });

        // The four list gestures reach the rows and the values.
        assert_eq!(
            resolve(&app, press(KeyCode::Down)),
            Some(Action::CursorDown)
        );
        assert_eq!(
            resolve(&app, press(KeyCode::Right)),
            Some(Action::AdjustRight)
        );
        // And the globals it would otherwise shadow are swallowed: `1` is a tab jump
        // everywhere else, and a settings screen that changed tab under the player
        // would not be modal at all.
        assert_eq!(resolve(&app, press(KeyCode::Char('1'))), None);
        assert_eq!(resolve(&app, press(KeyCode::Tab)), None);

        assert_eq!(
            resolve(&app, press(KeyCode::Char('s'))),
            Some(Action::CloseModal)
        );
        assert_eq!(resolve(&app, press(KeyCode::Esc)), Some(Action::CloseModal));
    }

    /// The lateral pair reaches the menu vocabulary too, which is what the title's
    /// Settings screen is driven by.
    #[test]
    fn a_menu_screen_understands_the_lateral_pair() {
        assert_eq!(resolve_menu(press(KeyCode::Left)), Some(MenuAction::Left));
        assert_eq!(resolve_menu(press(KeyCode::Right)), Some(MenuAction::Right));
    }

    #[test]
    fn while_help_is_up_it_captures_the_keys_and_closes_on_question_or_esc() {
        let mut app = session();
        app.modal = Some(Modal::Help);
        // Its own key and `Esc` both close it.
        assert_eq!(
            resolve(&app, press(KeyCode::Char('?'))),
            Some(Action::CloseModal)
        );
        assert_eq!(resolve(&app, press(KeyCode::Esc)), Some(Action::CloseModal));
        // A ring key is swallowed while the modal is up, not acted on behind it.
        assert_eq!(resolve(&app, press(KeyCode::Tab)), None);
        assert_eq!(resolve(&app, press(KeyCode::Char('1'))), None);
        // But `Ctrl-C` still outranks even a modal.
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(resolve(&app, ctrl_c), Some(Action::Quit));
    }

    /// The compression dialog owns the keyboard the same way Help does, but it has
    /// gestures to hand back rather than only a way out.
    ///
    /// **The five keys it answers are four it did not have to invent.** `←/→` is
    /// *adjust the value under the cursor* everywhere in UI.md §9 and a spinner is
    /// exactly that; `Enter` and `Esc` are the acts every modal shares. Only `a` is
    /// new. That is the dividend of naming the list gestures after movements instead
    /// of after screens.
    #[test]
    fn the_compression_dialog_captures_the_keyboard_and_answers_its_five_keys() {
        let mut app = session();
        app.modal = Some(Modal::Compress {
            material: Material::Iron,
            direction: Conversion::Compress,
            units: 3,
        });

        assert_eq!(
            resolve(&app, press(KeyCode::Left)),
            Some(Action::AdjustLeft)
        );
        assert_eq!(
            resolve(&app, press(KeyCode::Right)),
            Some(Action::AdjustRight)
        );
        assert_eq!(
            resolve(&app, press(KeyCode::Char('a'))),
            Some(Action::AdjustMax)
        );
        assert_eq!(resolve(&app, press(KeyCode::Enter)), Some(Action::Confirm));
        assert_eq!(resolve(&app, press(KeyCode::Esc)), Some(Action::CloseModal));

        // Everything else is swallowed rather than leaking to the Inventory screen
        // behind it — including the globals, which is what "modal" means. `c` would
        // otherwise open a second dialog over the first.
        assert_eq!(resolve(&app, press(KeyCode::Tab)), None);
        assert_eq!(resolve(&app, press(KeyCode::Char('q'))), None);
        assert_eq!(resolve(&app, press(KeyCode::Char('c'))), None);
        assert_eq!(resolve(&app, press(KeyCode::Char('?'))), None);

        // `Ctrl-C` still outranks even a modal — the one key that is never captured.
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(resolve(&app, ctrl_c), Some(Action::Quit));
    }

    /// A session on the Upgrades screen, where the sub-tab binding lives.
    fn upgrading() -> App {
        let mut app = session();
        app.screen = Screen::Upgrades;
        app
    }

    /// The default binding: `Shift` **and** the arrow, together.
    ///
    /// A bare `←` must stay unclaimed here — UI.md §9 leaves the lateral axis free on
    /// this screen precisely so the sub-tab can own it, and a decode that dropped the
    /// modifier would take a key the player expects to do nothing and make it change
    /// what they are looking at.
    #[test]
    fn the_default_sub_tab_binding_is_the_shifted_arrows_and_only_those() {
        let app = upgrading();
        let shifted = |code| KeyEvent::new(code, KeyModifiers::SHIFT);

        assert_eq!(
            resolve(&app, shifted(KeyCode::Right)),
            Some(Action::NextSubTab)
        );
        assert_eq!(
            resolve(&app, shifted(KeyCode::Left)),
            Some(Action::PrevSubTab)
        );
        assert_eq!(
            resolve(&app, press(KeyCode::Left)),
            None,
            "an unshifted arrow switched sub-tabs"
        );
    }

    /// Help renders the binding from config (UI.md §6.11), and so does the keymap —
    /// which is the half that was missing: the field existed and was displayed for two
    /// phases before anything was bound to it.
    #[test]
    fn each_configured_binding_is_the_one_that_answers() {
        for (choice, prev, next) in [
            (SubTabKeys::HL, KeyCode::Char('h'), KeyCode::Char('l')),
            (SubTabKeys::Brackets, KeyCode::Char('['), KeyCode::Char(']')),
        ] {
            let mut app = upgrading();
            app.config.sub_tab_keys = choice;

            assert_eq!(resolve(&app, press(prev)), Some(Action::PrevSubTab));
            assert_eq!(resolve(&app, press(next)), Some(Action::NextSubTab));
            // And the default's own keys stop answering, or a rebind would add a
            // binding rather than move one.
            assert_eq!(
                resolve(&app, KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT)),
                None,
                "{choice:?} left the shifted arrows bound"
            );
        }
    }

    /// **Gated on the screen, and that is why two of the three choices are safe at
    /// all.** `h` and `[` are ordinary characters: claimed globally they would be
    /// swallowed on every screen for a gesture only one of them has.
    #[test]
    fn the_sub_tab_keys_are_not_claimed_on_other_screens() {
        let mut app = session();
        app.config.sub_tab_keys = SubTabKeys::HL;
        app.screen = Screen::Mines;

        assert_eq!(resolve(&app, press(KeyCode::Char('h'))), None);
        assert_eq!(
            resolve(&app, KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT)),
            Some(Action::AdjustRight),
            "the Mines dial lost its own arrow"
        );
    }

    /// The dip box binds the two list gestures, `Enter`, and the frame's own `n` —
    /// and swallows everything else, which is what makes it modal.
    #[test]
    fn the_dip_box_takes_the_arrows_enter_and_its_own_printed_key() {
        let mut app = session();
        app.modal = Some(Modal::Dip { to: 7, buy: false });

        assert_eq!(
            resolve(&app, press(KeyCode::Left)),
            Some(Action::AdjustLeft)
        );
        assert_eq!(
            resolve(&app, press(KeyCode::Right)),
            Some(Action::AdjustRight)
        );
        assert_eq!(resolve(&app, press(KeyCode::Enter)), Some(Action::Confirm));
        // `n` is the key the §6.7 frame prints, and it declines outright — the same
        // act every other modal spells `Esc`, so it decodes to the same action.
        assert_eq!(
            resolve(&app, press(KeyCode::Char('n'))),
            Some(Action::CloseModal)
        );
        assert_eq!(resolve(&app, press(KeyCode::Esc)), Some(Action::CloseModal));
        // Swallowed, not passed through: `q` would otherwise quit from under the box.
        assert_eq!(resolve(&app, press(KeyCode::Char('q'))), None);
        assert_eq!(resolve(&app, press(KeyCode::Tab)), None);
    }

    #[test]
    fn the_demo_toast_key_is_gone_now_that_the_tick_speaks() {
        // `t` was scaffolding: it existed so the overlay path could be exercised
        // before the tick produced real events. It is bound to nothing now, and this
        // asserts the removal rather than merely deleting the old test — a stand-in
        // that comes back is exactly what a removed test stops catching.
        let app = session();
        assert_eq!(resolve(&app, press(KeyCode::Char('t'))), None);
    }

    /// **The seam the Stats history bug lived in, and this is the test that crosses
    /// it.**
    ///
    /// Everything on either side was covered: `app` moved the cursor when handed
    /// [`Action::CursorDown`], and `screen::stats` drew whatever row it was told was
    /// selected. Both suites passed for a screen where `↑` decoded to nothing at all,
    /// because neither of them asks a *key* for an answer. A binding does not exist
    /// until this function says it does, so this is where a binding is asserted.
    #[test]
    fn the_arrows_scroll_the_history_on_the_stats_screen() {
        let mut app = session();
        app.screen = Screen::Stats;

        assert_eq!(resolve(&app, press(KeyCode::Up)), Some(Action::CursorUp));
        assert_eq!(
            resolve(&app, press(KeyCode::Down)),
            Some(Action::CursorDown)
        );
        // And the screen's own letter still answers beside them rather than being
        // displaced by the two arms above it.
        assert_eq!(
            resolve(&app, press(KeyCode::Char('p'))),
            Some(Action::OpenPrestige)
        );
    }

    /// `Home` reaches the same action the Levels roadmap decodes it to, from the second
    /// screen that has a *where you actually are* to go back to.
    #[test]
    fn home_answers_on_both_screens_that_have_somewhere_to_return_to() {
        for screen in [Screen::Stats, Screen::Levels] {
            let mut app = session();
            app.screen = screen;
            assert_eq!(
                resolve(&app, press(KeyCode::Home)),
                Some(Action::JumpToCurrent),
                "{screen:?} did not answer Home"
            );
        }

        // And nowhere else: the key is a screen binding, so the four screens with no
        // such place must leave it unclaimed rather than swallowing it.
        for screen in [
            Screen::Mine,
            Screen::Mines,
            Screen::Inventory,
            Screen::Upgrades,
        ] {
            let mut app = session();
            app.screen = screen;
            assert_eq!(
                resolve(&app, press(KeyCode::Home)),
                None,
                "{screen:?} claimed Home"
            );
        }
    }

    /// Contextual like every other screen binding: the Mine screen owns no list, so an
    /// arrow there stays unclaimed rather than moving something the player cannot see.
    #[test]
    fn the_history_arrows_are_not_claimed_on_a_screen_with_no_list() {
        let mut app = session();
        app.screen = Screen::Mine;

        assert_eq!(resolve(&app, press(KeyCode::Up)), None);
        assert_eq!(resolve(&app, press(KeyCode::Down)), None);
    }

    /// A key event of a given kind — the third argument the release path turns on.
    fn of_kind(code: KeyCode, kind: KeyEventKind) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind)
    }

    #[test]
    fn space_swings_the_pickaxe_only_on_the_mine_screen() {
        let mut app = session();
        assert_eq!(
            resolve(&app, press(KeyCode::Char(' '))),
            Some(Action::MinePressed)
        );

        // Everywhere else it is unbound: `Space` is the Mine screen's own key
        // (UI.md §9), and a global one would swallow it on five screens that have
        // nothing to do with it.
        app.screen = Screen::Upgrades;
        assert_eq!(resolve(&app, press(KeyCode::Char(' '))), None);
    }

    #[test]
    fn an_auto_repeat_of_space_reads_exactly_like_a_fresh_press() {
        // The kitty protocol reports a held key as `Repeat`; the legacy encoding
        // reports it as another `Press`. Both must reach the same action, or the hold
        // window would be refreshed on one terminal and not the other.
        let app = session();
        assert_eq!(
            resolve(&app, of_kind(KeyCode::Char(' '), KeyEventKind::Repeat)),
            Some(Action::MinePressed)
        );
    }

    #[test]
    fn releasing_space_stops_the_swing() {
        let app = session();
        assert_eq!(
            resolve(&app, of_kind(KeyCode::Char(' '), KeyEventKind::Release)),
            Some(Action::MineReleased)
        );
    }

    #[test]
    fn a_release_of_any_other_key_means_nothing_at_all() {
        // **The price of `event` no longer filtering key kinds.** Without the branch
        // at the top of `resolve`, every binding here would fire twice on a terminal
        // that reports releases: once down, once up. `Tab` is the loudest — two tabs
        // per keystroke — so it is the one pinned.
        let app = session();
        for code in [
            KeyCode::Tab,
            KeyCode::Char('q'),
            KeyCode::Char('?'),
            KeyCode::Char('1'),
            KeyCode::Esc,
        ] {
            assert_eq!(
                resolve(&app, of_kind(code, KeyEventKind::Release)),
                None,
                "{code:?} acted on the way up as well as on the way down"
            );
        }
    }

    #[test]
    fn a_release_outranks_an_open_modal() {
        // The one place a release beats "a modal captures every key". A modal cannot
        // use a release, and swallowing this one would leave the pickaxe swinging
        // behind the box until the hold window ran out on its own.
        let mut app = session();
        app.modal = Some(Modal::Help);
        assert_eq!(
            resolve(&app, of_kind(KeyCode::Char(' '), KeyEventKind::Release)),
            Some(Action::MineReleased)
        );
        // And a *press* is still swallowed by it, which is what makes the line above
        // an exception rather than a hole.
        assert_eq!(resolve(&app, press(KeyCode::Char(' '))), None);
    }
}

/// The dev key's tests, gated like the key.
///
/// A module and not an attribute per test: they name `Modal::Dev` and
/// `Action::OpenDevMenu`, neither of which exists in a build with
/// `debug_assertions` off — so `cargo test --release` would fail to *compile* the
/// suite rather than skip a feature that is not there.
#[cfg(all(test, debug_assertions))]
mod dev_tests {
    use super::tests::{press, session};
    use super::*;

    /// The inner half of the dev gate: the branch is compiled into this build, and the
    /// key is still dead until the session asked for it.
    #[test]
    fn the_dev_key_is_unbound_until_the_session_asks_for_it() {
        let plain = session();
        assert_eq!(resolve(&plain, press(KeyCode::Char('`'))), None);

        let enabled = session().with_dev(true);
        assert_eq!(
            resolve(&enabled, press(KeyCode::Char('`'))),
            Some(Action::OpenDevMenu)
        );
    }

    /// It opens from every screen, like `?` and unlike a screen binding — and on the
    /// Mine screen in particular, where `Space` is the only other thing bound.
    #[test]
    fn the_dev_key_opens_from_any_screen() {
        for screen in Screen::ALL {
            let mut app = session().with_dev(true);
            app.screen = screen;
            assert_eq!(
                resolve(&app, press(KeyCode::Char('`'))),
                Some(Action::OpenDevMenu),
                "the dev key is shadowed on {screen:?}"
            );
        }
    }

    /// The menu captures the list gestures and closes on either of its two keys.
    #[test]
    fn the_dev_menu_captures_the_list_gestures_and_closes_on_its_own_key() {
        let mut app = session().with_dev(true);
        app.modal = Some(Modal::Dev);

        for (code, expected) in [
            (KeyCode::Up, Action::CursorUp),
            (KeyCode::Down, Action::CursorDown),
            (KeyCode::Left, Action::AdjustLeft),
            (KeyCode::Right, Action::AdjustRight),
            (KeyCode::Enter, Action::Confirm),
            (KeyCode::Esc, Action::CloseModal),
            (KeyCode::Char('`'), Action::CloseModal),
        ] {
            assert_eq!(resolve(&app, press(code)), Some(expected), "{code:?}");
        }

        // And it swallows the ring, which is what makes it modal: `Tab` behind an open
        // box would change the screen the box is drawn over.
        assert_eq!(resolve(&app, press(KeyCode::Tab)), None);
    }
}
