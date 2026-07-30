//! Turning a run into text, and back.
//!
//! This is **half** of the save system, and the half that has no I/O in it. The
//! core writes and reads a `String`; the HMAC that signs it, the temp-file-then-
//! rename that writes it atomically, the `.bak` it falls back to and the clock it
//! stamps belong to the front-end (`docs/SYSTEMS.md`). Keeping the split here is
//! what lets every test in this module run without a filesystem — and what keeps
//! the core's "pure, deterministic, no I/O" contract true.
//!
//! ## What is in the file
//!
//! One JSON object: a [`version`](SAVE_VERSION), the [`GameState`], and the
//! front-end's own configuration, whatever that turns out to be. The state is
//! written by the serde derives on the types themselves — this module invents no
//! format, it only wraps one.
//!
//! **The version is inside the payload, not beside it.** `docs/SYSTEMS.md`
//! sketches an envelope of `{version, data, mac}` where the signature covers `data`
//! alone; a version living out there would be the one field a tamperer could edit
//! freely, and it is precisely the field that decides which migration runs. Inside,
//! it is signed like everything else. Nothing stops the envelope from repeating it
//! later as a routing hint — the reverse, hoisting it out of the signature, cannot
//! be undone.
//!
//! ## What is *not* in the file
//!
//! Anything derivable. The unlocked worlds are a function of the level, the mine's
//! block count is a function of its grid, and neither is stored — a second copy is
//! an invariant to maintain by hand, and a save is the worst place to keep one.
//!
//! ## Compact, not pretty
//!
//! [`to_json`] writes the compact form. Pretty-printing a mine puts every one of
//! its two hundred cells on a line of its own, which is not what "human-debuggable"
//! bought us; the readability that matters is that the keys are *words* —
//! `"iron"`, `"Netherite"`, `"richness_setting"` — and that survives the whitespace
//! being gone.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

use crate::game::GameState;

/// The schema this build writes, and the newest it can read.
///
/// Bumped whenever the shape of what is written changes — a renamed field, a
/// restructured one, a new configuration entry. Adding a field that
/// [`serde(default)`](https://serde.rs/field-attrs.html#default) can fill in is the
/// one change that does *not* need a bump, because an old file already answers for
/// it.
pub const SAVE_VERSION: u32 = 1;

/// The key the version is written under, named once so the reader and the writer
/// cannot disagree about it.
const VERSION_FIELD: &str = "version";

/// What a load hands back: the run, and the preferences it was written with.
///
/// **Generic over the configuration**, and the core never learns what it is. A
/// palette, a glyph set and a number format are the front-end's business, but
/// `docs/SYSTEMS.md` settles that they live in the *save* — one file, one path, no
/// XDG handling — so they have to cross this module. A type parameter is how they
/// cross it without the core growing an opinion: Rust resolves `C` at compile time,
/// so the front-end gets its own type back, already typed, with no untyped blob to
/// convert and no error case for the conversion failing.
///
/// **Public fields, unusually for this crate.** Everywhere else a private field
/// guards an invariant — an inventory never stores a zero, a dial never passes its
/// ceiling. This struct guards nothing: it is the pair a load produces, and both
/// halves already defend themselves. Accessors would only be ceremony.
#[derive(Debug, Deserialize)]
pub struct Save<C> {
    /// The run, exactly as it was written.
    pub state: GameState,
    /// The front-end's preferences, exactly as it handed them over.
    pub config: C,
}

/// The shape that goes *out*, which borrows rather than owns.
///
/// A caller autosaves every ten seconds while still playing, so it cannot hand its
/// [`GameState`] over to a writer and hope to get it back. Serialising from
/// references costs the caller nothing and is why [`to_json`] is a function taking
/// `&GameState` rather than a method on [`Save`].
#[derive(Serialize)]
struct Written<'run, C> {
    version: u32,
    state: &'run GameState,
    config: &'run C,
}

/// Writes a run and its configuration as one JSON document.
///
/// Fails only if `C` itself refuses to serialise — the game's own types cannot,
/// which is what the round-trip tests across the crate establish.
pub fn to_json<C: Serialize>(state: &GameState, config: &C) -> Result<String, SaveError> {
    let written = Written {
        version: SAVE_VERSION,
        state,
        config,
    };
    serde_json::to_string(&written).map_err(SaveError::Unwritable)
}

/// Reads back what [`to_json`] wrote, migrating it forward if it is older.
///
/// **The version is read before the state is typed**, which is the whole reason
/// this goes through a [`Value`] first: a file written by an older build does not
/// have to *parse* as today's [`GameState`], it only has to be migratable into one.
/// Typing it first would refuse exactly the files migration exists to rescue.
///
/// The four ways it can refuse are the four things that can be wrong with a file,
/// and they are deliberately distinguishable — the recovery screen offers a backup,
/// and "your save is from a newer version of the game" is not the same message as
/// "your save is damaged".
pub fn from_json<C: DeserializeOwned>(text: &str) -> Result<Save<C>, SaveError> {
    let document: Value = serde_json::from_str(text).map_err(SaveError::Unreadable)?;

    let found = document
        .get(VERSION_FIELD)
        .and_then(Value::as_u64)
        .ok_or(SaveError::MissingVersion)?;

    // A version that does not fit a `u32` is from the future too: versions only
    // ever count up, so there is no reading of a huge one that this build wrote.
    let from = match u32::try_from(found) {
        Ok(version) if version <= SAVE_VERSION => version,
        _ => {
            return Err(SaveError::FromTheFuture {
                found,
                current: SAVE_VERSION,
            });
        }
    };

    let migrated = migrate(document, from)?;
    let save: Save<C> = serde_json::from_value(migrated).map_err(SaveError::Unreadable)?;

    // Parsing proves the file has the right *shape*; it proves nothing about the
    // rules. Serde writes private fields directly, so every check the game makes on
    // the way in — a dial under its ceiling, a level on the ladder, a grid the size
    // it claims — is bypassed here and nowhere else. This is the one place to make
    // them again.
    save.state
        .validate()
        .map_err(|invariant| SaveError::Invalid { invariant })?;

    Ok(save)
}

/// Brings a document written at version `from` up to [`SAVE_VERSION`].
///
/// **Takes and returns the document**, because that is the shape a chain wants: a
/// file at v1 read by a v4 build travels 1 → 2 → 3 → 4, each step rewriting what
/// the next one expects, and no step ever has to know about more than its own
/// successor. Written as one step per version rather than one function per *pair*
/// of versions, which would grow as the square of the schema's age.
///
/// There is no step yet, and there cannot be: [`SAVE_VERSION`] is 1, the first
/// schema this game has had, so a document behind it claims a version no build ever
/// wrote. That is a refusal, not a migration.
fn migrate(save: Value, from: u32) -> Result<Value, SaveError> {
    if from < SAVE_VERSION {
        return Err(SaveError::UnknownVersion { version: from });
    }
    Ok(save)
}

/// Why a save could not be written or read.
///
/// **Its own error type, and not a [`CoreError`](crate::error::CoreError) variant.**
/// That enum is `Copy` and a [`serde_json::Error`] is not, but the real reason is
/// what `error.rs` says it is for: an operation *the player can legitimately get
/// wrong* — spending what they do not have, buying a level that does not exist. A
/// damaged file is nothing the player did at the keyboard, and no message about
/// their inventory helps them. The two are different genera and stay apart.
#[derive(Debug)]
pub enum SaveError {
    /// The configuration refused to serialise. The game's own state cannot.
    Unwritable(serde_json::Error),
    /// The text is not JSON, or is JSON that does not describe a run.
    Unreadable(serde_json::Error),
    /// Nothing in the document says which schema it is, so nothing can be assumed
    /// about the rest of it.
    MissingVersion,
    /// The file was written by a newer build than this one.
    ///
    /// Carries both numbers, per `error.rs`'s doctrine that a refusal should be
    /// actionable: "update the game" is only sayable if the message knows the file
    /// is ahead rather than merely wrong.
    FromTheFuture {
        /// The version the file claims.
        found: u64,
        /// The newest version this build understands.
        current: u32,
    },
    /// The file claims a version that has no path forward to [`SAVE_VERSION`].
    UnknownVersion {
        /// The version the file claims.
        version: u32,
    },
    /// The file parses, but describes a run the rules could not have produced.
    ///
    /// Carries a `&'static str` naming the invariant rather than a
    /// [`CoreError`](crate::error::CoreError). That enum answers the *player* —
    /// "you are 40 Iron short" — and here there is nothing for them to do about a
    /// grid that is the wrong size; the recovery screen says the same thing
    /// whichever field it was. The string is for whoever reads the bug report.
    Invalid {
        /// Which rule the file breaks, in words.
        invariant: &'static str,
    },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unwritable(error) => write!(f, "the run could not be written: {error}"),
            Self::Unreadable(error) => write!(f, "the save could not be read: {error}"),
            Self::MissingVersion => write!(f, "the save does not say which version it is"),
            Self::FromTheFuture { found, current } => write!(
                f,
                "the save is version {found}, and this build reads up to {current}"
            ),
            Self::UnknownVersion { version } => {
                write!(f, "no version {version} was ever written by this game")
            }
            Self::Invalid { invariant } => write!(f, "the save describes a run where {invariant}"),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Unwritable(error) | Self::Unreadable(error) => Some(error),
            Self::MissingVersion
            | Self::FromTheFuture { .. }
            | Self::UnknownVersion { .. }
            | Self::Invalid { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Input;
    use crate::mine_kind::MineKind;
    use std::time::{Duration, SystemTime};

    /// Stands in for the front-end's real configuration: this module is generic
    /// over it precisely so it never has to know.
    #[derive(Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
    struct Preferences {
        palette: String,
        ascii_only: bool,
    }

    fn preferences() -> Preferences {
        Preferences {
            palette: "dim".to_owned(),
            ascii_only: true,
        }
    }

    /// A run that has actually been played, so the file under test is the file a
    /// player would write.
    fn a_run() -> GameState {
        let mut state = GameState::new(99, SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000));
        for _ in 0..200 {
            state.tick(Input { space_held: true });
        }
        state
    }

    fn written(state: &GameState) -> String {
        match to_json(state, &preferences()) {
            Ok(text) => text,
            Err(error) => unreachable!("a run should be writable: {error}"),
        }
    }

    fn read(text: &str) -> Save<Preferences> {
        match from_json::<Preferences>(text) {
            Ok(save) => save,
            Err(error) => unreachable!("a written save should load: {error}\n{text}"),
        }
    }

    #[test]
    fn a_run_and_its_preferences_survive_the_round_trip() {
        let state = a_run();
        let save = read(&written(&state));

        assert_eq!(save.config, preferences());
        assert_eq!(written(&save.state), written(&state));
    }

    /// The configuration crosses the core untouched. It is the one field here the
    /// core cannot check the meaning of, so what it owes is exact carriage.
    #[test]
    fn the_configuration_crosses_unchanged() {
        let state = a_run();
        let mut previous = preferences();
        for palette in ["dim", "bright", ""] {
            previous.palette = palette.to_owned();
            let text = match to_json(&state, &previous) {
                Ok(text) => text,
                Err(error) => unreachable!("preferences should be writable: {error}"),
            };
            assert_eq!(read(&text).config, previous);
        }
    }

    /// The reloaded run continues the same history, through the save this time
    /// rather than through a bare [`GameState`]: the generator's position rides in
    /// the file, so the two go on ticking identically.
    #[test]
    fn a_reloaded_save_ticks_like_the_run_it_came_from() {
        let mut original = a_run();
        let mut reloaded = read(&written(&original)).state;

        for _ in 0..200 {
            original.tick(Input { space_held: true });
            reloaded.tick(Input { space_held: true });
        }

        assert_eq!(written(&reloaded), written(&original));
    }

    #[test]
    fn the_version_is_written_inside_the_document() {
        let text = written(&a_run());
        assert!(
            text.starts_with(&format!(r#"{{"{VERSION_FIELD}":{SAVE_VERSION},"#)),
            "the version must be in the payload the front-end signs: {text}"
        );
    }

    /// **The golden save.** A fixed run writes one exact document, pinned here in
    /// full — the same device as `rng`'s golden vector, aimed at the format instead
    /// of the sequence.
    ///
    /// What it catches is a change nothing else notices: rename a private field,
    /// reorder an enum, swap a map for a hashed one, and every test in this crate
    /// still passes while every save already on a player's disk stops loading. If
    /// this test fails, the question is not "what is the new text?" but "what did we
    /// just do to every existing save?" — and the answer is a
    /// [`SAVE_VERSION`] bump with a migration.
    #[test]
    fn the_written_shape_is_pinned() {
        let state = GameState::new(1, SystemTime::UNIX_EPOCH + Duration::from_secs(1_000));

        assert_eq!(
            written(&state),
            concat!(
                r#"{"version":1,"state":{"#,
                r#""player":{"pickaxe":{"tier":"Wooden","enchants":{}},"level":1,"#,
                r#""experience":0,"inventory":{},"prestige":0,"xp_carry":0},"#,
                r#""mine":{"kind":"Stone","size_level":0,"richness_level":0,"#,
                r#""richness_setting":0,"grid":[["Stone","Stone","Cobblestone"],"#,
                r#"["Stone","Stone","Stone"],["Stone","Stone","Stone"]],"#,
                r#""break_progress":0.0,"target":null},"visited":{},"boost_charges":0,"#,
                // `unclaimed` joined the document without a `SAVE_VERSION` bump, which
                // is the one change this constant's own doc says does not need one: a
                // file written before it exists reads back as an empty set, and an
                // empty set is the *truth* about such a file rather than a default
                // covering for missing information — those runs were paid at the tick.
                r#""unclaimed":[],"#,
                r#""active_boost":null,"auto_common_progress":0,"auto_value_progress":0,"#,
                r#""yield_carry":[],"rng":{"seed":[234,216,29,114,93,38,16,78,137,156,"#,
                r#"59,248,66,206,120,46,186,211,3,218,153,151,210,194,18,2,86,172,115,"#,
                r#"102,251,27],"stream":0,"word_pos":9},"last_seen":1000},"#,
                r#""config":{"palette":"dim","ascii_only":true}}"#,
            )
        );
    }

    #[test]
    fn a_document_that_is_not_json_is_refused() {
        assert!(matches!(
            from_json::<Preferences>("not a save at all"),
            Err(SaveError::Unreadable(_))
        ));
    }

    /// A version-less document says nothing about its own shape, so nothing about
    /// it can be assumed — including that today's reader would be right about it.
    #[test]
    fn a_document_without_a_version_is_refused() {
        let text = written(&a_run()).replacen(VERSION_FIELD, "vresion", 1);
        assert!(matches!(
            from_json::<Preferences>(&text),
            Err(SaveError::MissingVersion)
        ));
    }

    /// A newer build's save is refused rather than half-read. Its own message
    /// exists because the answer for the player is "update the game", which no
    /// other refusal here can say.
    #[test]
    fn a_save_from_a_newer_build_is_refused_by_name() {
        let text = written(&a_run()).replacen(
            &format!(r#""{VERSION_FIELD}":{SAVE_VERSION}"#),
            &format!(r#""{VERSION_FIELD}":{}"#, u64::from(SAVE_VERSION) + 1),
            1,
        );

        match from_json::<Preferences>(&text) {
            Err(SaveError::FromTheFuture { found, current }) => {
                assert_eq!(found, u64::from(SAVE_VERSION) + 1);
                assert_eq!(current, SAVE_VERSION);
            }
            other => unreachable!("a newer save must be named as such: {other:?}"),
        }
    }

    /// An absurd version is the same refusal, not a panic and not a wrap: versions
    /// only count up, so nothing this build wrote is out there claiming `u64::MAX`.
    #[test]
    fn a_version_too_large_to_name_is_still_from_the_future() {
        let text = written(&a_run()).replacen(
            &format!(r#""{VERSION_FIELD}":{SAVE_VERSION}"#),
            &format!(r#""{VERSION_FIELD}":{}"#, u64::MAX),
            1,
        );

        assert!(matches!(
            from_json::<Preferences>(&text),
            Err(SaveError::FromTheFuture { .. })
        ));
    }

    /// The migration hook, exercised at the only version that reaches it. Version 1
    /// is the first schema, so anything below it is a claim no build ever made —
    /// and the loop that will one day walk 1 → 2 → 3 starts here, refusing to guess.
    #[test]
    fn a_version_behind_the_first_schema_is_refused() {
        let text = written(&a_run()).replacen(
            &format!(r#""{VERSION_FIELD}":{SAVE_VERSION}"#),
            &format!(r#""{VERSION_FIELD}":0"#),
            1,
        );

        assert!(matches!(
            from_json::<Preferences>(&text),
            Err(SaveError::UnknownVersion { version: 0 })
        ));
    }

    /// A save that is missing a field of the run itself is a damaged file, not a
    /// migration: today's version promised today's shape.
    #[test]
    fn a_document_missing_part_of_the_run_is_refused() {
        let text = written(&a_run()).replacen(r#""visited":"#, r#""vistied":"#, 1);
        assert!(matches!(
            from_json::<Preferences>(&text),
            Err(SaveError::Unreadable(_))
        ));
    }

    /// The mines left behind travel with the run, holes and all. A save that
    /// dropped them would hand a returning player a free batch reset of every mine
    /// they had dug — the exact exploit `MECHANICS.md` keeps `visited` for.
    #[test]
    fn the_mines_left_behind_are_saved_too() {
        // The run has dug its Stone mine; walking out of it is what moves that
        // half-dug grid, holes and all, into the map the save has to carry.
        let mut state = a_run();
        assert!(state.select_mine(MineKind::Coal).is_ok());

        let reloaded = read(&written(&state)).state;
        let before = match state.mine(MineKind::Stone) {
            Some(mine) => mine.remaining_count(),
            None => unreachable!("the Stone mine was entered and left"),
        };
        let after = match reloaded.mine(MineKind::Stone) {
            Some(mine) => mine.remaining_count(),
            None => unreachable!("the Stone mine must survive the save"),
        };

        assert_eq!(after, before);
        assert!(
            before < state.current_mine().capacity(),
            "the fixture must actually have dug the mine it left"
        );
    }

    /// **The wiring, end to end.** A document that parses perfectly and still
    /// describes an impossible run is refused — here a richness dial pushed above
    /// the ceiling that was bought for it, which in play is a
    /// [`CoreError::RichnessAboveCeiling`](crate::error::CoreError) and in a file is
    /// nothing at all until this check runs.
    ///
    /// The individual invariants are tested where they live, with the types that own
    /// them; what is under test here is that `from_json` asks at all.
    #[test]
    fn a_run_the_rules_could_not_have_produced_is_refused() {
        let text =
            written(&a_run()).replacen(r#""richness_setting":0"#, r#""richness_setting":3"#, 1);

        match from_json::<Preferences>(&text) {
            Err(SaveError::Invalid { invariant }) => {
                assert!(
                    invariant.contains("dial"),
                    "unhelpful diagnosis: {invariant}"
                );
            }
            other => unreachable!("an impossible run must be refused: {other:?}"),
        }
    }

    /// Validation runs *after* the file parses, so a save can be perfectly typed
    /// and still be rejected — and the message has to say which of the two it was.
    /// The recovery screen offers the same backup either way, but the bug report
    /// does not.
    #[test]
    fn a_damaged_file_and_an_impossible_run_are_told_apart() {
        let damaged = from_json::<Preferences>("{\"version\":1}");
        let impossible = from_json::<Preferences>(&written(&a_run()).replacen(
            r#""level":1"#,
            r#""level":0"#,
            1,
        ));

        assert!(matches!(damaged, Err(SaveError::Unreadable(_))));
        assert!(matches!(impossible, Err(SaveError::Invalid { .. })));
    }

    /// Every refusal is renderable: the front-end shows one of these on the
    /// recovery screen, and a message that says nothing is a screen that helps
    /// nobody.
    #[test]
    fn every_refusal_says_something() {
        let refusals = [
            SaveError::MissingVersion,
            SaveError::FromTheFuture {
                found: 9,
                current: SAVE_VERSION,
            },
            SaveError::UnknownVersion { version: 0 },
            SaveError::Invalid {
                invariant: "a mine's grid is not the size its level says it is",
            },
        ];
        for refusal in refusals {
            assert!(!refusal.to_string().is_empty(), "{refusal:?} says nothing");
        }
    }

    /// The two refusals that wrap a `serde_json::Error` render it inside their own
    /// sentence and forward it as their [`source`](std::error::Error::source); the
    /// self-contained ones have no source. `source` is what lets a front-end log the
    /// underlying parser error beneath the friendly line on the recovery screen.
    #[test]
    fn the_wrapped_errors_render_and_expose_their_source() {
        use std::error::Error;

        // Any failed parse hands back a real `serde_json::Error` to wrap; the shape
        // it failed on does not matter, only that it is one of serde_json's own.
        let wrapped = || match serde_json::from_str::<i32>("not a number") {
            Ok(number) => unreachable!("that is not a number: {number}"),
            Err(error) => error,
        };

        let unwritable = SaveError::Unwritable(wrapped());
        let unreadable = SaveError::Unreadable(wrapped());

        assert!(unwritable.to_string().contains("could not be written"));
        assert!(unreadable.to_string().contains("could not be read"));
        assert!(unwritable.source().is_some());
        assert!(unreadable.source().is_some());
        assert!(SaveError::MissingVersion.source().is_none());
    }
}
