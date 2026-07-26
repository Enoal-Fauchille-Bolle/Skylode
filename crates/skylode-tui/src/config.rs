//! Front-end configuration — the player preferences the UI reads while drawing.
//!
//! These are the fields the Settings screen edits (UI.md §6.10) and, in the full
//! game, the ones stored **inside the signed save** so there is no config file to
//! desync from it. Phase 2 holds only the one binding the Help and Settings screens
//! need to render — the sub-tab switch — with its default; the rest arrives with the
//! save (phase 7). It lives in the front-end, never the core: a keybinding is not a
//! game rule, and the determinism contract must not see it.

/// The key that switches Upgrades sub-tabs — three choices, `⇧←→` by default.
///
/// Configurable because `←`/`→` already means "adjust the value under the cursor"
/// everywhere (UI.md §9), so the lateral axis is free for this to own — but only if
/// the player who dislikes the default can move it. The default is AZERTY-safe (the
/// arrows do not move) and footer-discoverable as `⇧←→`; the alternatives are the
/// vi-style `h`/`l` and the bracket pair `[`/`]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SubTabKeys {
    /// `Shift+←` / `Shift+→`, printed `⇧← ⇧→`.
    #[default]
    ShiftArrows,
    /// The vi pair `h` / `l`.
    #[allow(dead_code, reason = "constructed by Settings editing, phase 7")]
    HL,
    /// The bracket pair `[` / `]`.
    #[allow(dead_code, reason = "constructed by Settings editing, phase 7")]
    Brackets,
}

impl SubTabKeys {
    /// How the binding prints in a footer or help line.
    ///
    /// Help renders this **from config, not from the default** (UI.md §6.11): an aid
    /// that showed `⇧←→` while the player had chosen `h`/`l` would teach a key that
    /// does nothing, which is worse than no aid.
    pub fn label(self) -> &'static str {
        match self {
            Self::ShiftArrows => "⇧← ⇧→",
            Self::HL => "h  l",
            Self::Brackets => "[  ]",
        }
    }
}

/// The front-end preferences held while a session runs.
///
/// One field today; it is a struct rather than a bare `SubTabKeys` so the colour
/// mode, number format and the rest can join it in phase 7 without changing every
/// caller's shape.
#[derive(Clone, Debug, Default)]
pub struct Config {
    /// The Upgrades sub-tab switch binding.
    pub sub_tab_keys: SubTabKeys,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_binding_is_the_shift_arrows_the_footer_advertises() {
        assert_eq!(Config::default().sub_tab_keys, SubTabKeys::ShiftArrows);
        assert_eq!(SubTabKeys::ShiftArrows.label(), "⇧← ⇧→");
    }

    #[test]
    fn each_choice_prints_its_own_keys() {
        assert_eq!(SubTabKeys::HL.label(), "h  l");
        assert_eq!(SubTabKeys::Brackets.label(), "[  ]");
    }
}
