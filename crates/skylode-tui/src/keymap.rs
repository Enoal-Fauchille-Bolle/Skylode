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

use crate::{action::Action, app::App, overlay::Modal};

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
    //    would otherwise claim, which is what "modal" means. Help closes on its own
    //    key (`?`) or `Esc`, and swallows all others so nothing leaks to the screen
    //    behind it.
    if let Some(modal) = app.modal {
        return match modal {
            Modal::Help => match key.code {
                KeyCode::Char('?') | KeyCode::Esc => Some(Action::CloseModal),
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

    // 4. Fall through to whatever the current screen wants.
    app.screen.map_key(key)
}

#[cfg(test)]
mod tests {
    use skylode_core::game::GameState;

    use super::*;
    use crate::screen::Screen;

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
