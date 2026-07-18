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

use crate::{action::Action, app::App};

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

    // 2. A modal captures the rest. `Modal` has no variants yet, so this never
    //    fires — but the branch is where modal keymaps will hang, and having it
    //    here now is what stops the globals below from being wired as if modals
    //    did not exist.
    if let Some(modal) = app.modal {
        match modal {}
    }

    // 3. The global ring bindings.
    match key.code {
        KeyCode::Char('q') => return Some(Action::Quit),
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
    use super::*;
    use crate::screen::Screen;

    /// A plain key press, with no modifiers.
    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn tab_advances_the_ring_and_backtab_reverses_it() {
        let app = App::new();
        assert_eq!(resolve(&app, press(KeyCode::Tab)), Some(Action::NextScreen));
        assert_eq!(
            resolve(&app, press(KeyCode::BackTab)),
            Some(Action::PrevScreen)
        );
    }

    #[test]
    fn the_digit_keys_are_zero_based_tab_indices() {
        let app = App::new();
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
        let app = App::new();
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
        let app = App::new();
        assert_eq!(resolve(&app, press(KeyCode::Char('7'))), None);
    }

    #[test]
    fn q_and_ctrl_c_both_quit() {
        let app = App::new();
        assert_eq!(resolve(&app, press(KeyCode::Char('q'))), Some(Action::Quit));
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(resolve(&app, ctrl_c), Some(Action::Quit));
    }

    #[test]
    fn an_unbound_key_is_declined_rather_than_swallowed() {
        let app = App::new();
        assert_eq!(resolve(&app, press(KeyCode::Char('z'))), None);
    }
}
