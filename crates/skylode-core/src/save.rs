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
use crate::material::COMPRESSED_PREFIX;
use crate::tunables::RAW_PER_COMPRESSED;

/// The schema this build writes, and the newest it can read.
///
/// Bumped whenever the shape of what is written changes — a renamed field, a
/// restructured one, a new configuration entry. Adding a field that
/// [`serde(default)`](https://serde.rs/field-attrs.html#default) can fill in is the
/// one change that does *not* need a bump, because an old file already answers for
/// it.
pub const SAVE_VERSION: u32 = 3;

/// The schema that had no `auto_raw_credited`, and therefore the last one whose ore
/// could not be audited. See [`migrate`].
const VERSION_WITHOUT_AUTO_TOTAL: u32 = 1;

/// The last schema whose [`Boost`](crate::boost::Boost) carried only a remainder,
/// with no record of what it had been granted. See [`migrate`].
const VERSION_WITHOUT_BOOST_TOTAL: u32 = 2;

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
/// There are two steps. `1 → 2` gives a document the `auto_raw_credited` counter that
/// [`GameState`]'s ore audit reads; `2 → 3` gives a running boost the `granted_ticks`
/// total its gauge is a fraction of. Anything claiming a version below 1 claims a
/// schema no build ever wrote, and that is a refusal rather than a migration.
///
/// **The second step is `<=` where the first is `==`**, and the difference is which
/// question each is answering. `grandfather_auto_total` reconstructs a field for the
/// one version that lacked it, and a v1 file that has just run through it now *has*
/// that field — asking again would overwrite a value the previous step computed.
/// `grandfather_boost_total` is downstream of it: a v1 file needs it just as much as a
/// v2 one, because neither ever wrote the field. Chained steps are the reason to keep
/// each condition honest about the whole range it covers rather than only its own
/// predecessor.
fn migrate(mut save: Value, from: u32) -> Result<Value, SaveError> {
    if from < VERSION_WITHOUT_AUTO_TOTAL {
        return Err(SaveError::UnknownVersion { version: from });
    }
    if from == VERSION_WITHOUT_AUTO_TOTAL {
        grandfather_auto_total(&mut save);
    }
    if from <= VERSION_WITHOUT_BOOST_TOTAL {
        grandfather_boost_total(&mut save);
    }
    Ok(save)
}

/// The field [`grandfather_auto_total`] writes, named once so this module and
/// [`GameState`]'s derive cannot drift apart. A typo here would not fail to compile —
/// it would produce a document missing a field, which surfaces as a *damaged save* on
/// the one build that has to read it.
const AUTO_TOTAL_FIELD: &str = "auto_raw_credited";

/// Gives a version-1 document an `auto_raw_credited` equal to the ore it is already
/// holding.
///
/// **The value is a choice, and the two obvious ones are both wrong.** The counter
/// records what the auto-miner has paid out over the save's whole life, and a version-1
/// file simply does not contain that — an absence leaves no trace in
/// [`playtime`](GameState), so there is nothing to reconstruct it from and no number of
/// absences to assume.
///
/// - **Zero** would be a lie in the dangerous direction. A player who left the game for
///   a week and came back to a full purse would load a file whose ore exceeds every
///   ceiling the audit can build, and be told their honest save is impossible. Losing a
///   run to a guess is the outcome the whole audit is written to avoid.
/// - **[`u64::MAX`]** would saturate the ceiling and switch the audit off — not for one
///   load, but for that save forever, since the number is carried forward and only ever
///   grows.
///
/// So the migration **grandfathers the present**: whatever ore the file holds is
/// accepted as legitimately earned, and everything from that load onwards is audited
/// against real counters. It cannot refuse the file it is migrating — the allowance is
/// at least the holdings, by construction — and it does not blind the audit afterwards,
/// because the number stops moving except when the auto-miner actually pays.
///
/// **It concedes nothing to a tamperer**, which is worth stating because it looks like
/// it does. Someone who can edit a version-1 file to inflate its inventory holds the
/// signing key already, and a person holding the key can write a version-2 file with
/// any counter they like. The migration is generous to the past because the past is
/// unknowable, not because the check is weak.
///
/// Every step reads defensively and gives up quietly: a document whose shape is not
/// what this expects is about to fail to deserialise anyway, and inventing a diagnosis
/// here would only race the real one.
fn grandfather_auto_total(save: &mut Value) {
    let held = save
        .get("state")
        .and_then(|state| state.get("player"))
        .and_then(|player| player.get("inventory"))
        .and_then(Value::as_object)
        .map_or(0, |items| {
            items.iter().fold(0u64, |total, (key, count)| {
                let each = if key.starts_with(COMPRESSED_PREFIX) {
                    u64::from(RAW_PER_COMPRESSED)
                } else {
                    1
                };
                let held = count.as_u64().unwrap_or(0).saturating_mul(each);
                total.saturating_add(held)
            })
        });

    if let Some(state) = save.get_mut("state").and_then(Value::as_object_mut) {
        state.insert(AUTO_TOTAL_FIELD.to_owned(), Value::from(held));
    }
}

/// The two fields [`grandfather_boost_total`] reads and writes, named for
/// [`AUTO_TOTAL_FIELD`]'s reason: a typo compiles, and surfaces as a *damaged save*
/// on the one build that has to read the file.
const BOOST_REMAINING_FIELD: &str = "remaining_ticks";
const BOOST_TOTAL_FIELD: &str = "granted_ticks";

/// Gives a version-2 document's running boost a `granted_ticks` equal to what it has
/// left.
///
/// **Why the field appeared at all.** Charges stack by addition, so a boost fired
/// into twice is sixty seconds long; a gauge measured against the thirty-second
/// constant then reads full for the whole first charge. The denominator has to be the
/// boost's own total, and no version-2 file records it.
///
/// **Equality is the only honest reconstruction, and it is also a safe one.** The
/// information is simply absent — a v2 boost leaves no trace of how many charges went
/// into it — so nothing can be recovered, only chosen. Setting the total to the
/// remainder says *this boost is as full as it will ever be*: the gauge reopens at
/// 100 % and drains correctly over whatever is actually left, and the player loses at
/// most the knowledge that the bar should have started part-way down. The two
/// alternatives are both worse in the way [`grandfather_auto_total`]'s are. **Zero**
/// would fail [`Boost::validate`](crate::boost::Boost) on the spot — more left than
/// granted — and refuse a file the migration was written to rescue, which is the one
/// thing a migration must never do. **The constant** would be a guess that is wrong
/// for every stack and right only by coincidence for a single charge, and it can
/// land *below* the remainder, so it fails the same check on exactly the saves it
/// was meant to serve.
///
/// **A `null` boost is the common case and needs nothing.** The field lives inside
/// the boost object, so a save written with none has no object to repair — which is
/// also why this cannot be a plain `serde(default)`: the default would have to read
/// its sibling to be right, and serde never shows a field its neighbours.
///
/// Reads defensively and gives up quietly, per the last paragraph of
/// [`grandfather_auto_total`].
fn grandfather_boost_total(save: &mut Value) {
    let Some(boost) = save
        .get_mut("state")
        .and_then(|state| state.get_mut("active_boost"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };

    let remaining = boost
        .get(BOOST_REMAINING_FIELD)
        .and_then(Value::as_u64)
        .unwrap_or(0);
    boost.insert(BOOST_TOTAL_FIELD.to_owned(), Value::from(remaining));
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
    use crate::boost::Boost;
    use crate::game::Input;
    use crate::mine_kind::MineKind;
    use crate::tunables::BOOST_MULTIPLIER;
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
                r#"{"version":3,"state":{"#,
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
                // **`null`, which is why version 3's field does not appear here.**
                // `granted_ticks` lives *inside* the boost, so a run that has fired
                // none writes nothing new and this document is byte-identical to the
                // version-2 one but for the number at the front. That is the whole
                // hazard the `2 → 3` migration answers: the golden save cannot show
                // the change, so only `a_version_two_save_with_a_boost_running_still_loads`
                // stands between the bump and a file this test says nothing about.
                r#""active_boost":null,"auto_common_progress":0,"auto_value_progress":0,"#,
                r#""yield_carry":[],"rng":{"seed":[234,216,29,114,93,38,16,78,137,156,"#,
                r#"59,248,66,206,120,46,186,211,3,218,153,151,210,194,18,2,86,172,115,"#,
                r#"102,251,27],"stream":0,"word_pos":9},"#,
                // The three counters of core phase 11, added with **no
                // `SAVE_VERSION` bump and no `serde(default)`**, which looks like the
                // `unclaimed` precedent above and is its opposite on both halves.
                //
                // No bump, because a bump protects files already written and none
                // exist: the front-end owns the disk and has not been given it yet
                // (TUI phase 8), so a migration step could never run and its test
                // would describe an impossible situation.
                //
                // No default, because a default may stand in for an **absence** and
                // never for a **missing fact**. `unclaimed`'s empty set was *true* of
                // an older file — those runs paid at the tick, so nothing was waiting
                // — whereas a `0` here would be *false*: the player broke blocks, we
                // simply would not know how many.
                r#""blocks_broken":0,"playtime":0,"run_playtime":0,"#,
                // `auto_raw_credited`, and **this** one took the bump the three above
                // did not — the argument that spared them had one moving part, and it
                // moved. Files written by TUI phase 8 exist now, on this machine and on
                // anyone else's who has played, so a migration step both can and must
                // run. It is `1 → 2` in [`migrate`], and it grandfathers rather than
                // defaults for the same reason the note above refuses `serde(default)`:
                // a `0` would be *false* of an older file, and false in the direction
                // that accuses an honest save of holding ore it could not have earned.
                r#""auto_raw_credited":0,"#,
                // Version 3 added `granted_ticks` and took a bump for the same
                // reason `auto_raw_credited` did — files exist now. What is worth
                // recording is why `serde(default)` was refused a *third* time, on a
                // ground the two notes above do not cover: a default here would have
                // to equal a **sibling field** to be right, and serde never shows a
                // field its neighbours. `0` is not merely uninformative, it is the
                // one value `Boost::validate` refuses, so the shortcut would turn an
                // honest save into a damaged one.
                r#""last_seen":1000},"#,
                r#""config":{"palette":"dim","ascii_only":true}}"#,
            )
        );
    }

    /// A version-1 document, built from a version-2 one the only way it can be: take
    /// the field the bump added back out, and put the old number on it.
    ///
    /// The ore is inflated first, so the document is one the audit *would* refuse with
    /// a counter of zero. That is what makes the pair of tests below mean something —
    /// without it, both would pass whatever the migration did.
    fn a_version_one_document_holding(raw: u32) -> String {
        let text = written(&a_run());
        // Cut out `"auto_raw_credited":<n>,` whatever `n` is — a played run has already
        // banked some, which is the counter working rather than a fixture to pin.
        let key = format!(r#""{AUTO_TOTAL_FIELD}":"#);
        let at = match text.find(&key) {
            Some(at) => at,
            None => unreachable!("the field the bump added is not in the document: {text}"),
        };
        let end = match text[at..].find(',') {
            Some(comma) => at + comma + 1,
            None => unreachable!("the field the bump added is the last one: {text}"),
        };

        format!("{}{}", &text[..at], &text[end..])
            .replacen(
                &format!(r#""{VERSION_FIELD}":{SAVE_VERSION}"#),
                &format!(r#""{VERSION_FIELD}":{VERSION_WITHOUT_AUTO_TOTAL}"#),
                1,
            )
            .replacen(
                r#""inventory":{"#,
                // Both denominations, because the sum has to convert one of them. A
                // migration that counted a Compressed unit as *one* raw would
                // grandfather a total a hundredfold short of what the file holds, and
                // the load below would refuse the very save it was written to rescue —
                // so this is what makes that branch checked rather than merely run.
                &format!(r#""inventory":{{"compressed_diamond":{raw},"diamond":{raw},"#),
                1,
            )
    }

    /// **The migration's whole reason.** A version-1 file cannot say what its
    /// auto-miner paid out over its life, and a `0` would be a claim rather than an
    /// absence — the claim that a player who left the game running for a week came back
    /// to ore they could not have earned. So the ore already in the file is
    /// grandfathered, and the file loads.
    #[test]
    fn a_version_one_save_keeps_the_ore_the_counter_cannot_account_for() {
        let text = a_version_one_document_holding(50_000_000);
        match from_json::<Preferences>(&text) {
            Ok(save) => assert!(save.state.validate().is_ok()),
            Err(error) => unreachable!("a version-1 save must survive the bump: {error}\n{text}"),
        }
    }

    /// A version-2 document whose boost was still running, built the only way it can
    /// be: write today's document and put the old boost shape back into it.
    ///
    /// **The remainder is deliberately longer than one charge.** 900 ticks is 45
    /// seconds, which no single charge can produce — it is a stack — and it is what
    /// makes the fixture discriminating. Had the migration guessed
    /// [`BOOST_DURATION_TICKS`](crate::tunables::BOOST_DURATION_TICKS) instead of the
    /// remainder, the total would land *below* what the boost has left, and
    /// [`Boost::validate`](crate::boost::Boost) would refuse the file. A thirty-second
    /// fixture would pass under either rule and prove nothing.
    fn a_version_two_document_with_a_boost_of(remaining: u32) -> String {
        let text = written(&a_run());
        assert!(
            text.contains(r#""active_boost":null"#),
            "the fixture assumes the played run fires no boost: {text}"
        );

        text.replacen(
            &format!(r#""{VERSION_FIELD}":{SAVE_VERSION}"#),
            &format!(r#""{VERSION_FIELD}":{VERSION_WITHOUT_BOOST_TOTAL}"#),
            1,
        )
        .replacen(
            r#""active_boost":null"#,
            // The version-2 shape: a multiplier and a remainder, and no third field.
            &format!(
                r#""active_boost":{{"multiplier":{BOOST_MULTIPLIER},"{BOOST_REMAINING_FIELD}":{remaining}}}"#
            ),
            1,
        )
    }

    /// **The `2 → 3` migration's whole reason.** A boost was running when the file was
    /// written, and a version-2 document has no room to say how long it had been
    /// granted for. Without the step the file does not merely lose a number — it fails
    /// to parse, and the front-end shows a *damaged save* recovery screen for a
    /// perfectly good run.
    #[test]
    fn a_version_two_save_with_a_boost_running_still_loads() {
        let text = a_version_two_document_with_a_boost_of(900);

        let save = match from_json::<Preferences>(&text) {
            Ok(save) => save,
            Err(error) => unreachable!("a version-2 save must survive the bump: {error}\n{text}"),
        };

        let boost = match save.state.active_boost() {
            Some(boost) => boost,
            None => unreachable!("the migration dropped the boost instead of completing it"),
        };
        assert_eq!(boost.remaining_ticks(), 900, "the remainder was rewritten");
        assert_eq!(
            boost.granted_ticks(),
            900,
            "the total must equal the remainder: the gauge reopens full and drains \
             over what is actually left, which is the only reconstruction the file \
             supports"
        );
    }

    /// The other half, and without it the test above proves nothing: the same boost in
    /// a **version-3** document is refused. A file claiming today's schema must carry
    /// today's fields, so the load above is the migration doing work rather than serde
    /// quietly defaulting the gap away — which is exactly what `serde(default)` would
    /// have done, and would have handed the gauge a total of zero.
    #[test]
    fn the_same_boost_in_a_current_save_is_refused() {
        let text = a_version_two_document_with_a_boost_of(900).replacen(
            &format!(r#""{VERSION_FIELD}":{VERSION_WITHOUT_BOOST_TOTAL}"#),
            &format!(r#""{VERSION_FIELD}":{SAVE_VERSION}"#),
            1,
        );

        assert!(matches!(
            from_json::<Preferences>(&text),
            Err(SaveError::Unreadable(_))
        ));
    }

    /// A version-1 file needs the boost step as much as a version-2 one — neither
    /// schema ever wrote the field — which is why that step's condition is `<=` and
    /// not `==`. The two migrations chain, and this is the only test that walks both.
    #[test]
    fn a_version_one_save_with_a_boost_running_walks_both_steps() {
        let text = a_version_two_document_with_a_boost_of(900).replacen(
            &format!(r#""{VERSION_FIELD}":{VERSION_WITHOUT_BOOST_TOTAL}"#),
            &format!(r#""{VERSION_FIELD}":{VERSION_WITHOUT_AUTO_TOTAL}"#),
            1,
        );
        // The `1 → 2` step *inserts* `auto_raw_credited`, so the field today's writer
        // already put there has to come out or the document would carry it twice.
        let key = format!(r#""{AUTO_TOTAL_FIELD}":"#);
        let at = match text.find(&key) {
            Some(at) => at,
            None => unreachable!("the field the first bump added is not there: {text}"),
        };
        let end = match text[at..].find(',') {
            Some(comma) => at + comma + 1,
            None => unreachable!("the field the first bump added is the last one: {text}"),
        };
        let text = format!("{}{}", &text[..at], &text[end..]);

        match from_json::<Preferences>(&text) {
            Ok(save) => assert_eq!(
                save.state.active_boost().map(Boost::granted_ticks),
                Some(900),
                "the boost step was skipped for a file that predates it too"
            ),
            Err(error) => unreachable!("a version-1 save must survive both: {error}\n{text}"),
        }
    }

    /// The other half, and without it the test above proves nothing: the *same* ore in a
    /// version-2 document — one this build wrote, whose counter is therefore the truth —
    /// is refused. The migration is a concession to what an old file cannot know, not a
    /// hole in the audit.
    #[test]
    fn the_same_ore_in_a_current_save_is_refused() {
        let text = written(&a_run()).replacen(
            r#""inventory":{"#,
            r#""inventory":{"diamond":50000000,"#,
            1,
        );
        assert!(matches!(
            from_json::<Preferences>(&text),
            Err(SaveError::Invalid { .. })
        ));
    }

    /// A version below the first schema names a shape no build ever wrote, so there is
    /// nothing to migrate *from* and the file is refused rather than guessed at.
    #[test]
    fn a_version_below_the_first_schema_is_refused() {
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
