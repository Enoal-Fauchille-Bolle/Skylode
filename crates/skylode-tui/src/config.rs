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
//! ## Every preference is a closed enum, including the duration
//!
//! [`ToastDuration`] could have been a `u64` of seconds, and is not. The rule the
//! whole crate follows for cursors — *a row that is not there cannot be pointed at*
//! — applies just as well to a value: a ladder of four spelled as an enum makes
//! `toast_seconds: 0`, a preference that would silently switch every announcement
//! off, a state nobody can write down. The number reaches the outside world through
//! [`ToastDuration::seconds`], which is where a `Duration` is built from it.
//!
//! ## The field order is the screen's row order
//!
//! Not decoration: `docs/UI.md` §6.10's audit — *every config field, and no
//! game-state field* — is a comparison between this struct and that screen, and it is
//! only as cheap as the two orders agreeing. A field appended here out of order is a
//! field the next audit has to hunt for.
//!
//! ## Why the one remaining accessor carries `allow(dead_code)` and not `expect`
//!
//! The fields landed one session ahead of the screen that reads them, and while that
//! gap lasted every `label` here was called by the tests and by nothing else. The
//! screen exists now, so all but one of those attributes are gone;
//! [`NumberFormat::separator`] is the last, because the ~46 call sites that will read
//! it are a session away.
//!
//! `allow` and not `expect` is the same shape `palette::ColourMode` documented: a lint
//! expectation is checked **per compilation**, and a method like this is dead building
//! the binary while being live building the test harness — so `expect` is fulfilled in
//! one and unfulfilled in the other, and `--all-targets -D warnings` fails on the
//! second. `allow` is what stays quiet in both, and its reason names what removes it.
//!
//! ## Adding a field, later
//!
//! A new preference may carry
//! [`serde(default)`](https://serde.rs/field-attrs.html#default) and skip a
//! [`SAVE_VERSION`](skylode_core::save::SAVE_VERSION) bump — and here, unlike the
//! core's counters, that is honest: a preference missing from an older file means the
//! player never expressed one, which is exactly what the default says. A default may
//! stand in for an absence; it may not stand in for a fact we failed to record.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{palette::ColourMode, toast::TOAST_TTL};

/// How the mine key is read — the one preference that changes behaviour, not looks.
///
/// **An accessibility option, so it may not depend on the terminal.** A mode offered
/// only where the kitty keyboard protocol reports key releases would be a mode absent
/// from exactly the machines most likely to need it. `App::advance` therefore
/// implements [`PressToStart`](MiningInput::PressToStart) as a latch flipped on the
/// **rising edge** of the *"the key is down"* predicate the hold path already
/// computes, rather than on the key event itself: auto-repeat refreshes that
/// predicate without ever letting it fall, so holding the key toggles once instead of
/// flickering, whatever repeat rate the operating system is set to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MiningInput {
    /// Mine while the key is held, which is where `HOLD_WINDOW` comes from.
    #[default]
    Hold,
    /// One press starts mining, the next stops it.
    PressToStart,
}

impl MiningInput {
    /// The value column's text on the Settings screen.
    pub fn label(self) -> &'static str {
        match self {
            Self::Hold => "Hold",
            Self::PressToStart => "Press to start",
        }
    }
}

/// How a figure's thousands are grouped.
///
/// **Only the separator is a preference; abbreviation is not.** `docs/DECISIONS.md`
/// settles that numbers are exact and never shortened, because the composite
/// denomination exists so the player can check a purchase in their head and
/// *"I have 1.2M, the cost is 1.23M, can I?"* is unanswerable. So this enum offers
/// three ways to punctuate the same digits and no way to lose one.
///
/// The three exist because the two punctuation marks collide with a decimal point in
/// the locales this game is read in — a French reader and an English one disagree
/// about what `1,234` means — and `Plain` is the way out for anyone who would rather
/// read no punctuation at all than the wrong one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NumberFormat {
    /// `1 234 567` — an ASCII space, and the crate's default.
    #[default]
    Spaced,
    /// `1,234,567`.
    Comma,
    /// `1234567`, ungrouped.
    Plain,
}

impl NumberFormat {
    /// The value column's text — the format shown *as itself*, rather than named.
    ///
    /// A row reading `Spaced` would make the player picture the result; a row reading
    /// `1 234 567` **is** the result. The same trick the compression dialog plays with
    /// its denominations.
    pub fn label(self) -> &'static str {
        match self {
            Self::Spaced => "1 234 567",
            Self::Comma => "1,234,567",
            Self::Plain => "1234567",
        }
    }

    /// What to insert between groups of three, or [`None`] to insert nothing.
    ///
    /// An [`Option<char>`] and not a `&str` with an empty case, so that *"this format
    /// has no separator"* is a shape the caller has to handle rather than a string it
    /// might concatenate without noticing.
    #[allow(dead_code, reason = "awaiting the grouped(n, separator) signature")]
    pub fn separator(self) -> Option<char> {
        match self {
            Self::Spaced => Some(' '),
            Self::Comma => Some(','),
            Self::Plain => None,
        }
    }
}

/// How long an announcement stays on screen.
///
/// **A ladder and not a number.** See this module's header: an enum is what stops a
/// hand-edited `0` from switching every announcement off, and four rungs are enough
/// to span the two readers this has to serve — one who finds three seconds a
/// distraction and one who cannot finish the sentence in three seconds.
///
/// Nothing here bounds the Stats history: that buffer keeps every announcement
/// forever regardless (`toast.rs`), because *one buffer, two renderings* means expiry
/// is a question asked at the draw and never a deletion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ToastDuration {
    /// Two seconds.
    Brief,
    /// Three — the wireframe's figure, and what every toast used before this was a
    /// choice.
    #[default]
    Normal,
    /// Five.
    Long,
    /// Eight, for a reader who wants time to finish the line.
    Lingering,
}

impl ToastDuration {
    /// How many seconds this rung is worth.
    ///
    /// The default rung is **defined from [`TOAST_TTL`]** rather than spelling `3`
    /// again: that constant is the wireframe's figure and what every toast lasted
    /// before the duration was a preference, so a second copy here would be a way for
    /// a fresh install to stop behaving like the frames `docs/UI.md` draws without
    /// anything failing.
    fn seconds(self) -> u64 {
        match self {
            Self::Brief => 2,
            Self::Normal => TOAST_TTL.as_secs(),
            Self::Long => 5,
            Self::Lingering => 8,
        }
    }

    /// The same, as the [`Duration`] every `push` site wants.
    pub fn ttl(self) -> Duration {
        Duration::from_secs(self.seconds())
    }

    /// The value column's text, in the crate's own elapsed-time vocabulary (`3s`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Brief => "2s",
            Self::Normal => "3s",
            Self::Long => "5s",
            Self::Lingering => "8s",
        }
    }
}

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
/// Five fields, in the order `docs/UI.md` §6.10 draws them — see this module's header
/// for why that is a requirement and not a tidiness.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// How many colours to ask the terminal for.
    #[serde(default)]
    pub colour: ColourMode,
    /// Whether the mine key is held or toggled.
    #[serde(default)]
    pub mining_input: MiningInput,
    /// How a figure's thousands are punctuated.
    #[serde(default)]
    pub number_format: NumberFormat,
    /// How long an announcement stays up.
    #[serde(default)]
    pub toast_duration: ToastDuration,
    /// The Upgrades sub-tab switch binding.
    ///
    /// The one field with **no** `serde(default)`, and deliberately: it is the
    /// original, so it is present in every document ever written, and a default here
    /// would be a fallback for a case that cannot arise.
    pub sub_tab_keys: SubTabKeys,
}

impl Config {
    /// How long a toast this session pushes should live.
    ///
    /// A method on the whole config rather than a bare
    /// [`ToastDuration::ttl`], because that is the shape every call site wants: the
    /// pushers hold a `&Config` and should not have to know which field the answer
    /// comes out of.
    pub fn toast_ttl(&self) -> Duration {
        self.toast_duration.ttl()
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

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

    /// The defaults are the behaviour every phase before this one shipped.
    ///
    /// A preference that arrives with a *different* default is not a new setting but a
    /// silent change to every existing run, so this asserts the four together rather
    /// than trusting `#[default]` to have been put on the right variant four times.
    #[test]
    fn a_fresh_config_is_the_game_as_it_played_before_settings_existed() {
        let config = Config::default();
        assert_eq!(config.colour, ColourMode::Ansi256);
        assert_eq!(config.mining_input, MiningInput::Hold);
        assert_eq!(config.number_format, NumberFormat::Spaced);
        assert_eq!(config.toast_ttl(), Duration::from_secs(3));
    }

    /// Each row shows the reader the thing itself, not a name for it.
    #[test]
    fn the_number_format_rows_read_as_the_number_they_produce() {
        assert_eq!(NumberFormat::Spaced.label(), "1 234 567");
        assert_eq!(NumberFormat::Comma.label(), "1,234,567");
        assert_eq!(NumberFormat::Plain.label(), "1234567");
        assert_eq!(NumberFormat::Spaced.separator(), Some(' '));
        assert_eq!(NumberFormat::Comma.separator(), Some(','));
        assert_eq!(NumberFormat::Plain.separator(), None);
    }

    /// The ladder's labels and its durations are the same four rungs.
    ///
    /// Walked as a table rather than asserted one by one, because the failure this
    /// guards against is a rung whose *label* and *value* drift apart — `5s` that
    /// lasts eight seconds is a lie no single-value assertion would catch.
    #[test]
    fn every_toast_rung_says_how_long_it_actually_lasts() {
        for (rung, seconds) in [
            (ToastDuration::Brief, 2),
            (ToastDuration::Normal, 3),
            (ToastDuration::Long, 5),
            (ToastDuration::Lingering, 8),
        ] {
            assert_eq!(rung.ttl(), Duration::from_secs(seconds));
            assert_eq!(rung.label(), format!("{seconds}s"));
        }
    }

    /// The row names the gesture, not the mechanism.
    ///
    /// `Press to start` and not `Toggle`: the player is being told what to *do*, and
    /// the latch behind it is an implementation detail they never have to picture.
    #[test]
    fn the_mining_rows_name_the_gesture_rather_than_the_mechanism() {
        assert_eq!(MiningInput::Hold.label(), "Hold");
        assert_eq!(MiningInput::PressToStart.label(), "Press to start");
    }

    #[test]
    fn the_colour_rows_print_their_palette_size() {
        assert_eq!(ColourMode::Ansi256.label(), "256");
        assert_eq!(ColourMode::Ansi16.label(), "16");
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
    /// is a [`SAVE_VERSION`](skylode_core::save::SAVE_VERSION) bump with a migration,
    /// *unless* the change is a newly **added** preference, which
    /// `an_older_document_loads_with_the_new_preferences_defaulted` covers instead.
    #[test]
    fn the_written_config_is_pinned() {
        let default = written(&Config::default());
        assert!(
            default.ends_with(
                r#""config":{"colour":"Ansi256","mining_input":"Hold","number_format":"Spaced","toast_duration":"Normal","sub_tab_keys":"ShiftArrows"}}"#
            ),
            "{default}"
        );
        let dialled = Config {
            colour: ColourMode::Ansi16,
            mining_input: MiningInput::PressToStart,
            number_format: NumberFormat::Comma,
            toast_duration: ToastDuration::Lingering,
            sub_tab_keys: SubTabKeys::HL,
        };
        let text = written(&dialled);
        assert!(
            text.ends_with(
                r#""config":{"colour":"Ansi16","mining_input":"PressToStart","number_format":"Comma","toast_duration":"Lingering","sub_tab_keys":"HL"}}"#
            ),
            "{text}"
        );
    }

    /// The preferences cross the core untouched, through the real generic rather than
    /// through a stand-in: `save.rs` exercises `Save<C>` against a `Preferences` of
    /// its own, which proves the *mechanism* and cannot prove that **this** type fits
    /// it.
    ///
    /// Every field is varied away from its default in the same document, so a field
    /// that failed to travel could not be masked by the default it would fall back to.
    #[test]
    fn the_preferences_survive_a_round_trip_through_the_core() {
        for sub_tab_keys in [
            SubTabKeys::ShiftArrows,
            SubTabKeys::HL,
            SubTabKeys::Brackets,
        ] {
            for number_format in [
                NumberFormat::Spaced,
                NumberFormat::Comma,
                NumberFormat::Plain,
            ] {
                let config = Config {
                    colour: ColourMode::Ansi16,
                    mining_input: MiningInput::PressToStart,
                    number_format,
                    toast_duration: ToastDuration::Brief,
                    sub_tab_keys,
                };
                match save::from_json::<Config>(&written(&config)) {
                    Ok(read) => assert_eq!(read.config, config),
                    Err(error) => unreachable!("a written save should load: {error}"),
                }
            }
        }
    }

    /// **The reason these four fields needed no `SAVE_VERSION` bump.**
    ///
    /// A save written before they existed carried a `config` object holding
    /// `sub_tab_keys` alone. It still loads, and the four arrive at their defaults —
    /// which is the honest reading, because a preference absent from that file means
    /// the player never expressed one.
    ///
    /// The old document is built by cutting the new one back down rather than pasted
    /// in as a literal, so the rest of the save stays whatever the current format is:
    /// what is under test is the *config* object's tolerance, and a frozen fixture
    /// would start failing for reasons in the core.
    #[test]
    fn an_older_document_loads_with_the_new_preferences_defaulted() {
        let current = written(&Config {
            sub_tab_keys: SubTabKeys::Brackets,
            ..Config::default()
        });
        let (head, _) = match current.split_once(r#""config":{"#) {
            Some(halves) => halves,
            None => unreachable!("a written save should carry a config object: {current}"),
        };
        let older = format!(r#"{head}"config":{{"sub_tab_keys":"Brackets"}}}}"#);

        match save::from_json::<Config>(&older) {
            Ok(read) => assert_eq!(
                read.config,
                Config {
                    sub_tab_keys: SubTabKeys::Brackets,
                    ..Config::default()
                }
            ),
            Err(error) => unreachable!("a pre-phase-9 save should still load: {error}"),
        }
    }
}
