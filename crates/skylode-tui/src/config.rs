//! Front-end configuration — the player preferences the UI reads while drawing.
//!
//! These are the fields the Settings screen edits (UI.md §6.10) and the ones stored
//! **inside the signed save** so there is no config file to desync from it. It lives
//! in the front-end, never the core: a keybinding is not a game rule, and the
//! determinism contract must not see it.
//!
//! ## Why this type is serialisable at all
//!
//! [`Config`] is the `C` of [`Save<C>`](skylode_core::save::Save) — the type
//! parameter `docs/SYSTEMS.md` describes as *"the core carries it and never learns
//! what it is"*. [`from_json`](skylode_core::save::from_json) bounds that parameter on
//! [`DeserializeOwned`](serde::de::DeserializeOwned), so these derives are not a
//! convenience: without them the preferences have no way across the boundary, and
//! *"config lives in the save"* has nowhere to happen.
//!
//! ## The variant names are on-disk format
//!
//! Serde writes a unit variant as its own identifier, so [`SubTabKeys::HL`] is the
//! word `"HL"` in every save file. That is safe here for the reason the core's `Item`
//! keys are safe: what the player *reads* comes from [`SubTabKeys::label`], which is
//! a separate function — so the display can be reworded freely, and only a **rename
//! of the variant** is a format change. `the_written_config_is_pinned` is what makes
//! such a rename fail loudly instead of silently invalidating every save on disk.
//!
//! ## Adding a field, later
//!
//! Phase 9 brings four more. They may carry
//! [`serde(default)`](https://serde.rs/field-attrs.html#default) and skip a
//! [`SAVE_VERSION`](skylode_core::save::SAVE_VERSION) bump — and here, unlike the
//! core's counters, that is honest: a preference missing from an older file means the
//! player never expressed one, which is exactly what the default says. A default may
//! stand in for an absence; it may not stand in for a fact we failed to record.

use serde::{Deserialize, Serialize};

/// The key that switches Upgrades sub-tabs — three choices, `⇧←→` by default.
///
/// Configurable because `←`/`→` already means "adjust the value under the cursor"
/// everywhere (UI.md §9), so the lateral axis is free for this to own — but only if
/// the player who dislikes the default can move it. The default is AZERTY-safe (the
/// arrows do not move) and footer-discoverable as `⇧←→`; the alternatives are the
/// vi-style `h`/`l` and the bracket pair `[`/`]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SubTabKeys {
    /// `Shift+←` / `Shift+→`, printed `⇧← ⇧→`.
    #[default]
    ShiftArrows,
    /// The vi pair `h` / `l`.
    HL,
    /// The bracket pair `[` / `]`.
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
/// mode, number format and the rest can join it in phase 9 without changing every
/// caller's shape.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// The Upgrades sub-tab switch binding.
    pub sub_tab_keys: SubTabKeys,
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use skylode_core::{game::GameState, save};

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

    /// A run, fixed rather than fresh from the clock, so the text below is the same
    /// text on every machine.
    fn a_run() -> GameState {
        GameState::new(
            7,
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        )
    }

    fn written(config: &Config) -> String {
        match save::to_json(&a_run(), config) {
            Ok(text) => text,
            Err(error) => unreachable!("preferences should be writable: {error}"),
        }
    }

    /// **The golden config.** The same device as the core's
    /// `the_written_shape_is_pinned`, aimed at this crate's half of the document —
    /// and asserted against the *save* rather than against a bare `to_string`, so
    /// what is pinned is the text that actually reaches the disk.
    ///
    /// What it catches is a change nothing else notices: rename a field, rename a
    /// variant, and every other test in this crate still passes while every save
    /// already written stops loading. If this fails, the question is not "what is the
    /// new text?" but "what did we just do to every existing save?" — and the answer
    /// is a [`SAVE_VERSION`](skylode_core::save::SAVE_VERSION) bump with a migration.
    #[test]
    fn the_written_config_is_pinned() {
        assert!(
            written(&Config::default()).ends_with(r#""config":{"sub_tab_keys":"ShiftArrows"}}"#),
            "{}",
            written(&Config::default())
        );
        let vi = Config {
            sub_tab_keys: SubTabKeys::HL,
        };
        assert!(
            written(&vi).ends_with(r#""config":{"sub_tab_keys":"HL"}}"#),
            "{}",
            written(&vi)
        );
    }

    /// The preferences cross the core untouched, through the real generic rather than
    /// through a stand-in: `save.rs` exercises `Save<C>` against a `Preferences` of
    /// its own, which proves the *mechanism* and cannot prove that **this** type fits
    /// it.
    #[test]
    fn the_preferences_survive_a_round_trip_through_the_core() {
        for sub_tab_keys in [
            SubTabKeys::ShiftArrows,
            SubTabKeys::HL,
            SubTabKeys::Brackets,
        ] {
            let config = Config { sub_tab_keys };
            match save::from_json::<Config>(&written(&config)) {
                Ok(read) => assert_eq!(read.config, config),
                Err(error) => unreachable!("a written save should load: {error}"),
            }
        }
    }
}
