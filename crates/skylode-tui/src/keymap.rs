//! Where a keystroke becomes an intent.
//!
//! This is the one place that knows about `KeyCode`. Everything downstream speaks
//! [`Action`], which is what keeps [`crate::app::App::update`] testable without a
//! terminal. It is also where UI-EN.md §9's configurable sub-tab binding will
//! land: one function to change, not a `match` scattered across six screens.
//!
//! **Resolution order**, and it matters:
//! 1. `Ctrl-C` — always quits, even if a modal would otherwise capture the key.
//! 2. An open modal captures everything else (it is modal; that is the point).
//! 3. The global ring bindings.
//! 4. The active screen's contextual bindings.
//!
//! Globals are consulted *before* the screen so that `Tab` cannot be shadowed by
//! a screen that forgot the ring exists.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{action::Action, app::App, config::SubTabKeys, overlay::Modal, screen::Screen};

/// The digits that jump straight to a tab. Derived from the ring, so a seventh
/// screen does not need this constant edited — only the ring.
const FIRST_TAB_DIGIT: char = '1';

/// Translates a key press into an [`Action`], or `None` if nothing is bound.
///
/// Takes `&App` rather than just the screen because resolution is *contextual*:
/// the same key means different things depending on whether a modal is open.
pub fn resolve(app: &App, key: KeyEvent) -> Option<Action> {
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
    if let Some(modal) = app.modal {
        return match modal {
            // Help closes on its own key (`?`) or `Esc`.
            Modal::Help => match key.code {
                KeyCode::Char('?') | KeyCode::Esc => Some(Action::CloseModal),
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
        };
    }

    // 3. The global ring bindings.
    match key.code {
        KeyCode::Char('q') => return Some(Action::Quit),
        // `?` is global and printed in every footer, so it opens Help from anywhere.
        KeyCode::Char('?') => return Some(Action::OpenHelp),
        KeyCode::Tab => return Some(Action::NextScreen),
        KeyCode::BackTab => return Some(Action::PrevScreen),
        // A temporary demo binding so the overlay path can be exercised before
        // the tick produces real events (UI-EN.md §6.2).
        KeyCode::Char('t') => {
            return Some(Action::ShowToast(
                "Excavator!  +1 Compressed Iron".to_owned(),
            ));
        }
        KeyCode::Char(digit @ '1'..='6') => {
            let index = digit as usize - FIRST_TAB_DIGIT as usize;
            return Some(Action::SelectScreen(index));
        }
        _ => {}
    }

    // 4. The configurable sub-tab binding, before the screen is consulted.
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

    // 5. Fall through to whatever the current screen wants.
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
    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// A session to resolve keys against.
    ///
    /// `resolve` reads only which screen is open and whether a modal is stacked, so
    /// the run behind it is immaterial — but it has to be *some* run, and a fixed
    /// seed keeps it from being a different one each time the suite is run.
    fn session() -> App {
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
    fn q_and_ctrl_c_both_quit() {
        let app = session();
        assert_eq!(resolve(&app, press(KeyCode::Char('q'))), Some(Action::Quit));
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(resolve(&app, ctrl_c), Some(Action::Quit));
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
    fn the_demo_toast_key_is_still_bound_and_still_global() {
        // `t` is scaffolding: it exists so the overlay path can be exercised before
        // the tick produces real events, and it is meant to go once phase 7 does.
        // Pinned rather than left untested for exactly that reason — a temporary
        // binding nothing asserts is one that quietly becomes permanent, and this
        // test is where its removal gets noticed.
        let app = session();
        assert_eq!(
            resolve(&app, press(KeyCode::Char('t'))),
            Some(Action::ShowToast(
                "Excavator!  +1 Compressed Iron".to_owned()
            ))
        );

        // Global, so every tab answers the same — it is decided before the screens
        // are consulted, and the fall-through below it must not change that.
        for screen in Screen::ALL {
            let mut app = session();
            app.screen = screen;
            assert!(
                matches!(
                    resolve(&app, press(KeyCode::Char('t'))),
                    Some(Action::ShowToast(_))
                ),
                "{screen:?} did not answer the global demo key"
            );
        }
    }
}
