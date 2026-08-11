//! The save file: the half of the save system that touches a disk.
//!
//! `skylode-core`'s [`skylode_core::save`] module turns a run into a [`String`]
//! and back, and stops there — deliberately, since a core that opened files could not
//! keep its "pure, deterministic, no I/O" contract. It closed with one sentence:
//! *"The HMAC, the atomic write, the `.bak` recovery and the clock reading stay
//! outside core."* This is outside core.
//!
//! ## The seam is a path, and that is not the seam [`Events`] needed
//!
//! [`Events`] is a trait because [`EventHandler`] **cannot run** outside a terminal —
//! its thread calls `event::poll`, which fails without a tty and takes the thread with
//! it — so a loop naming the concrete type was unreachable from a test by
//! construction. The disk is not that. A temporary directory is a real filesystem,
//! cheap and portable, so the tests here write real bytes and read them back.
//!
//! An in-memory fake would be worse than unnecessary. What this module has to get
//! right *is* filesystem behaviour — that a rename is indivisible, that a `.bak`
//! rotates, that a truncated file is refused — and a fake would have to **simulate**
//! those, which would leave the tests asserting one author's model of a filesystem
//! rather than the filesystem. Only the lookup of *where* the file lives stays
//! uncovered, for [`main`]'s own reason: it reads the environment.
//!
//! [`Events`]: crate::event::Events
//! [`EventHandler`]: crate::event::EventHandler
//! [`main`]: crate

mod envelope;
mod key;

use std::{
    fmt, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use directories::ProjectDirs;
use skylode_core::{
    game::GameState,
    save::{self, Save, SaveError},
};
use tempfile::NamedTempFile;

use crate::config::Config;

/// The save's name inside its directory.
const SAVE_FILE: &str = "save.json";

/// The backup's, beside it. A suffix rather than a directory of its own, so the two
/// files share a parent and one [`fs::rename`] can turn either into the other.
const BACKUP_FILE: &str = "save.json.bak";

/// The two files, and the directory they share.
///
/// **The directory is a field and not `primary.parent()`.** The atomic write needs it
/// twice — to create it, and to put the temporary file *inside* it, since a
/// [`persist`](NamedTempFile::persist) across filesystems is a copy rather than a
/// rename — and `parent()` answers an [`Option`] that cannot be [`None`] here. Storing
/// it removes an error case that could never fire instead of inventing a message for
/// one.
#[derive(Clone, Debug)]
pub struct SaveSlots {
    /// The directory both files live in.
    dir: PathBuf,
    /// `dir/save.json` — the file a session loads and writes.
    primary: PathBuf,
    /// `dir/save.json.bak` — the previous [`primary`](Self::primary), rotated aside by
    /// the last successful write.
    backup: PathBuf,
}

impl SaveSlots {
    /// The two slots inside `dir`.
    ///
    /// Takes the directory rather than finding it, which is what makes every rule in
    /// this module reachable from a test: [`location`] is the only caller that has to
    /// ask the platform, and the tests hand over a temporary directory instead.
    pub fn in_dir(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            primary: dir.join(SAVE_FILE),
            backup: dir.join(BACKUP_FILE),
        }
    }

    /// Where the session's own save lives.
    pub fn primary(&self) -> &Path {
        &self.primary
    }

    /// Where the last save that was replaced lives.
    ///
    /// Read by the recovery screen, and **only when the player asks for it**:
    /// `docs/UI.md` §8.3 makes restoring the backup an edge the player walks, not a
    /// fallback this module takes on its own.
    pub fn backup(&self) -> &Path {
        &self.backup
    }
}

/// Where this platform keeps the save, or nothing if it cannot say.
///
/// **The one function here no test covers**, for [`main`](crate)'s own reason: it
/// reads the environment, and mutating the environment from a test is `unsafe` in
/// edition 2024 and racy against every other test in the process. It is kept to a
/// single expression so that what is untested is the lookup and nothing else.
///
/// **[`None`] starts the game without persistence** rather than refusing to launch
/// (`docs/DECISIONS.md`) — a player who cannot save should still be able to play. It
/// is an [`Option`] at the edge and *not* a null [`SaveSlots`] that quietly writes
/// nowhere: nothing in this module can be asked to save into thin air, because
/// without a `SaveSlots` there is no way to ask. Same shape as
/// [`GameState::resume`](skylode_core::game::GameState::resume), whose [`None`] *is*
/// the "no screen at all" case.
///
/// It has exactly one caller, [`main`](crate), for the same reason
/// [`seed_from`](crate::session::seed_from) does: reading the environment is the
/// outside's job, and the reading is spent immediately on a [`Session`] that then
/// carries the answer for the rest of the run.
///
/// [`Session`]: crate::session::Session
pub fn location() -> Option<SaveSlots> {
    ProjectDirs::from("", "", "skylode").map(|dirs| SaveSlots::in_dir(dirs.data_dir()))
}

/// When the file at `path` was last written, or nothing if that cannot be asked.
///
/// **The one number the recovery screen cannot get any other way.** §6.3 offers
/// *"Restore the backup — saved 8 seconds ago"*, and that age is what the player's
/// decision is actually about: a backup from four seconds ago costs them nothing,
/// one from last week costs them a session. It cannot come from the file's own
/// [`last_seen`](GameState::last_seen), because at that point the backup has not been
/// verified — `docs/UI.md` §8.3 checks it only *after* the player asks for it — and
/// reading a `last_seen` out of an unverified file would be trusting the one thing on
/// screen that is under suspicion.
///
/// The modification time is honest here for a reason particular to this module: the
/// backup is produced by [`fs::rename`], which moves a directory entry and leaves the
/// content's timestamp alone. So this answers *when that run was written*, not when it
/// was rotated aside.
///
/// **Every failure is [`None`] rather than an error.** A missing file, a filesystem
/// that does not record modification times, a platform that refuses the call — none of
/// them is a reason to withhold the screen, and the caller's only use for the answer is
/// one line of text it can leave out.
pub fn written_at(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

/// Writes the run, atomically, rotating the previous save into the backup.
///
/// **Four steps, and the order is the contract:**
///
/// 1. the directory is created if it is not there — the one thing this module
///    *makes*, and a fresh install has no `~/.local/share/skylode` yet;
/// 2. the sealed text goes into a temporary file **in that same directory**, which is
///    what keeps the last step a rename rather than a copy;
/// 3. [`sync_all`](std::fs::File::sync_all) — without it the operating system is free
///    to let the rename reach the disk before the bytes do, and a power cut would
///    leave a correctly-named empty file. It costs one flush of a few kilobytes,
///    twice a minute;
/// 4. two renames: the old primary becomes the backup, then the temporary file
///    becomes the primary. Each is indivisible, so **the primary is never a
///    half-written file** — it is only ever the target of a rename.
///
/// **`now` is injected and no clock is read here**, the rule the core follows for the
/// same reason. Moving [`last_seen`](GameState::last_seen) is this function's job
/// rather than the caller's precisely because it has to happen *before* the run is
/// serialised: a caller that touched afterwards would write the previous mark and
/// pay the player twice for the same seconds.
///
/// **The rotation is blind** — the primary is not re-read and re-verified before it
/// becomes the backup. Checking would cost a read plus an HMAC on every autosave to
/// defend against a player editing their save while the game is running, which that
/// same player defeats by editing it while the game is closed.
pub fn save(
    slots: &SaveSlots,
    state: &mut GameState,
    config: &Config,
    now: SystemTime,
) -> Result<(), PersistError> {
    state.touch(now);
    let sealed = envelope::seal(&save::to_json(state, config).map_err(PersistError::Rejected)?);

    fs::create_dir_all(&slots.dir).map_err(PersistError::Io)?;
    let mut temporary = NamedTempFile::new_in(&slots.dir).map_err(PersistError::Io)?;
    temporary
        .write_all(sealed.as_bytes())
        .map_err(PersistError::Io)?;
    temporary.as_file().sync_all().map_err(PersistError::Io)?;

    match fs::rename(&slots.primary, &slots.backup) {
        // The first save of a run has no primary to rotate. That is not a failure,
        // and it is the only `NotFound` this step can produce.
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        other => other.map_err(PersistError::Io)?,
    }

    temporary
        .persist(&slots.primary)
        .map_err(|failure| PersistError::Io(failure.error))?;
    Ok(())
}

/// Reads **one** slot, and reports rather than repairs.
///
/// `docs/DECISIONS.md` keeps both files exactly as they are, so nothing here falls
/// back to the backup, deletes a bad file or clamps a value it dislikes. The caller
/// asks for a path and is told what is at it; `docs/UI.md` §8.3 turns *"restore the
/// backup"* into a keypress, which means the choice belongs to the player and the
/// state machine, not to a loader deciding on their behalf.
///
/// **A missing file is [`Ok(None)`](Option) and not an error.** A fresh install is
/// the ordinary opening of this game, and folding it in with the failures would make
/// the recovery screen the first thing every new player sees.
pub fn load(path: &Path) -> Result<Option<Save<Config>>, PersistError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(PersistError::Io(error)),
    };

    // Not UTF-8 is a fact about the *content*, not about reaching it, so it belongs
    // with the other ways a file can fail to be an envelope rather than with the
    // permissions and the missing directories. `fs::read_to_string` would have folded
    // it into an `io::Error` and lost that distinction.
    let text = String::from_utf8(bytes).map_err(|_| PersistError::Damaged)?;

    save::from_json::<Config>(&envelope::open(&text)?)
        .map(Some)
        .map_err(refusal)
}

/// Which refusal the core's own answer becomes.
///
/// Only [`FromTheFuture`](SaveError::FromTheFuture) is lifted out, and it is lifted
/// because it changes the **screen**: every other refusal here offers the backup,
/// while a save from a newer build says *"update the game"* — and must not offer the
/// backup at all, since a backup written by that same newer build is from the future
/// too. The rest keep the core's [`SaveError`] inside
/// [`Rejected`](PersistError::Rejected): one branch, and a sentence precise enough for
/// a bug report.
fn refusal(error: SaveError) -> PersistError {
    match error {
        SaveError::FromTheFuture { found, current } => {
            PersistError::FromTheFuture { found, current }
        }
        other => PersistError::Rejected(other),
    }
}

/// Why a save could not be written or read.
///
/// **The count is decided by the screens rather than by the causes.** `docs/UI.md`
/// §8.3 routes a failed load to one of three places — the recovery screen with the
/// backup offered, the same screen with nothing left to offer, or an "update the
/// game" message — so two causes share a variant exactly when they share a
/// destination. That is why every way an envelope can be malformed is one
/// [`Damaged`](Self::Damaged): unparseable, truncated and badly encoded are three
/// diagnoses of one sentence.
///
/// **"There is no file" is not here.** A fresh install is not a failure, so it is an
/// [`Ok(None)`](Option) from [`load`] — an [`Option`] at the edge, the same shape
/// [`GameState::resume`](skylode_core::game::GameState::resume) uses for "no screen at
/// all".
#[derive(Debug)]
pub enum PersistError {
    /// The bytes could not be reached: a permission refused, a directory that is a
    /// file, a disk that has gone away.
    ///
    /// **The backup answers nothing here.** Both files share a directory, so whatever
    /// stopped one will stop the other.
    Io(io::Error),
    /// The text is not an envelope: not JSON, not UTF-8, cut short by an interrupted
    /// write, or carrying a `mac` that is not sixty-four hexadecimal digits.
    ///
    /// Nothing can be said about what the file once held, so the recovery screen
    /// offers the backup and says the save is damaged.
    Damaged,
    /// The envelope is well formed and its signature does not match the payload.
    ///
    /// The file was edited, or a disk corrupted it in a way that still parses. This is
    /// the refusal `docs/DECISIONS.md` gives **no** *"continue anyway"*: loading data
    /// that failed its HMAC is exactly what a hand-editor needs.
    Tampered,
    /// The file was written by a newer build than this one.
    ///
    /// Signed correctly, so it is neither damaged nor edited — and the backup is no
    /// help, being from the same newer build. The only useful sentence is *"update the
    /// game"*, which is why this is a variant rather than a
    /// [`SaveError`](SaveError::FromTheFuture) inside [`Rejected`](Self::Rejected).
    FromTheFuture {
        /// The version the file claims.
        found: u64,
        /// The newest version this build understands.
        current: u32,
    },
    /// The core refused the payload.
    ///
    /// On the way **in** this is the interesting one: the signature checked out and
    /// the run described is still one the rules could not have produced — a migration
    /// bug, a disk that flipped a digit past the MAC, or a tamperer who found the key
    /// and did not also satisfy [`validate`]. On the way **out** it is [`Config`]
    /// refusing to serialise, which its derives make impossible; one variant covers
    /// both rather than inventing a second for a case that cannot happen.
    ///
    /// [`validate`]: skylode_core::game::GameState
    Rejected(SaveError),
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "the save file could not be reached: {error}"),
            Self::Damaged => write!(f, "the save file is damaged"),
            Self::Tampered => write!(f, "the save file does not match its checksum"),
            Self::FromTheFuture { found, current } => write!(
                f,
                "the save is version {found}, and this build reads up to {current}"
            ),
            Self::Rejected(error) => write!(f, "the save was refused: {error}"),
        }
    }
}

impl std::error::Error for PersistError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Rejected(error) => Some(error),
            Self::Damaged | Self::Tampered | Self::FromTheFuture { .. } => None,
        }
    }
}

/// Broken saves, built by the module that owns the key.
///
/// **Test-only, and `pub(crate)` on purpose.** The session state machine's tests have
/// to walk every branch of `docs/UI.md` §8.3, which means producing a file that fails
/// its signature and one that claims a version from the future — and neither can be
/// written from outside this module, because the key is deliberately not reachable
/// from anywhere else. Handing out two named fixtures is what keeps that true: nothing
/// outside `persist` gets [`envelope::seal`], only two files it already knows how to
/// describe.
#[cfg(test)]
pub(crate) mod fixture {
    use std::{fs, path::Path};

    use skylode_core::save::SAVE_VERSION;

    use super::envelope;

    /// Reads the file, or gives up loudly — a fixture that cannot be read is a broken
    /// test rather than a case under test.
    fn text_at(path: &Path) -> String {
        match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => unreachable!("{} should be readable: {error}", path.display()),
        }
    }

    fn write(path: &Path, text: String) {
        if let Err(error) = fs::write(path, text) {
            unreachable!("{} should be writable: {error}", path.display());
        }
    }

    /// Edits the payload without re-signing it, which is what a hand-editor leaves
    /// behind: a well-formed envelope whose `mac` no longer matches.
    pub(crate) fn tamper(path: &Path) {
        write(
            path,
            text_at(path).replacen(r#"{"data":"{"#, r#"{"data":"X"#, 1),
        );
    }

    /// Re-signs the payload claiming the next version up.
    ///
    /// **Signed correctly**, which is the whole point: this is what a newer build's own
    /// write looks like to this one, so the loader must refuse it for being from the
    /// future rather than for being damaged.
    pub(crate) fn from_the_future(path: &Path) {
        let payload = match envelope::open(&text_at(path)) {
            Ok(payload) => payload,
            Err(error) => unreachable!("the fixture should be a valid save: {error}"),
        };
        let bumped = payload.replace(
            &format!("\"version\":{SAVE_VERSION}"),
            &format!("\"version\":{}", SAVE_VERSION + 1),
        );
        assert_ne!(bumped, payload, "the fixture carried no version to bump");
        write(path, envelope::seal(&bumped));
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, time::Duration};

    use skylode_core::{game::Input, save::SAVE_VERSION};
    use tempfile::TempDir;

    use super::*;

    /// A fixed instant, so a written file is the same file on every machine.
    const NOW: SystemTime = SystemTime::UNIX_EPOCH;

    /// A run that has actually been played, so what reaches the disk is what a player
    /// would write rather than a fresh struct.
    fn a_run() -> GameState {
        let mut state = GameState::new(99, NOW);
        for _ in 0..200 {
            state.tick(Input { space_held: true });
        }
        state
    }

    /// A temporary directory and the two slots inside it.
    ///
    /// The [`TempDir`] is returned alongside because dropping it deletes the tree —
    /// so a test that let it go would be reading a directory that no longer exists.
    fn slots() -> (TempDir, SaveSlots) {
        let dir = match TempDir::new() {
            Ok(dir) => dir,
            Err(error) => unreachable!("a temporary directory should be creatable: {error}"),
        };
        let slots = SaveSlots::in_dir(dir.path());
        (dir, slots)
    }

    fn write(slots: &SaveSlots, state: &mut GameState) {
        if let Err(error) = save(slots, state, &Config::default(), NOW) {
            unreachable!("a run should be writable: {error}");
        }
    }

    fn read(path: &Path) -> Save<Config> {
        match load(path) {
            Ok(Some(save)) => save,
            other => unreachable!("{} should hold a save: {other:?}", path.display()),
        }
    }

    fn text_at(path: &Path) -> String {
        match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => unreachable!("{} should be readable: {error}", path.display()),
        }
    }

    /// The names in a directory, so a test can say *exactly* what is there.
    fn entries(dir: &Path) -> BTreeSet<String> {
        let read = match fs::read_dir(dir) {
            Ok(read) => read,
            Err(error) => unreachable!("{} should be listable: {error}", dir.display()),
        };
        read.filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect()
    }

    /// Rewrites a save's payload and **re-seals it with the real key**, which is what
    /// separates the two layers `docs/SYSTEMS.md` describes: this produces a file
    /// whose signature is perfect and whose contents the rules still refuse.
    fn reseal(path: &Path, edit: impl Fn(&str) -> String) {
        let payload = match envelope::open(&text_at(path)) {
            Ok(payload) => payload,
            Err(error) => unreachable!("the fixture should be a valid save: {error}"),
        };
        if let Err(error) = fs::write(path, envelope::seal(&edit(&payload))) {
            unreachable!("the fixture should be writable: {error}");
        }
    }

    /// The same edit the session's tests make, kept in one place: the fixtures
    /// `persist` hands out are the ones it tests itself against.
    fn corrupt(path: &Path) {
        super::fixture::tamper(path);
    }

    #[test]
    fn the_slots_are_the_save_and_its_backup_side_by_side() {
        let slots = SaveSlots::in_dir(Path::new("/somewhere"));
        assert_eq!(slots.primary(), Path::new("/somewhere/save.json"));
        assert_eq!(slots.backup(), Path::new("/somewhere/save.json.bak"));
        assert_eq!(slots.primary().parent(), slots.backup().parent());
    }

    /// The round trip, on real bytes: the run that comes back writes the same document
    /// as the run that went in, which is the core's own test for equality of state.
    #[test]
    fn a_run_survives_the_disk() {
        let (_dir, slots) = slots();
        let mut state = a_run();
        write(&slots, &mut state);

        let reloaded = read(slots.primary());
        assert_eq!(reloaded.config, Config::default());
        assert_eq!(
            save::to_json(&reloaded.state, &Config::default()).ok(),
            save::to_json(&state, &Config::default()).ok()
        );
    }

    /// The mark moves on the write, and it moves *before* the run is serialised — so
    /// the file carries the new one rather than the one the previous save wrote.
    #[test]
    fn a_write_stamps_the_run_with_the_moment_it_was_written() {
        let (_dir, slots) = slots();
        let mut state = a_run();
        let later = NOW + Duration::from_secs(3_600);

        if let Err(error) = save(&slots, &mut state, &Config::default(), later) {
            unreachable!("a run should be writable: {error}");
        }
        assert_eq!(state.last_seen(), later);
        assert_eq!(read(slots.primary()).state.last_seen(), later);
    }

    /// A fresh install has no `~/.local/share/skylode`. Making it is the one thing
    /// this module creates, and the only "repair" it performs.
    #[test]
    fn the_directory_is_made_when_it_is_not_there_yet() {
        let (dir, _) = slots();
        let nested = dir.path().join("not").join("there").join("yet");
        let slots = SaveSlots::in_dir(&nested);
        assert!(!nested.exists());

        write(&slots, &mut a_run());
        assert!(read(slots.primary()).state.last_seen() == NOW);
    }

    /// **What stands in for "a crash cannot leave a partial file".** The write goes
    /// through a temporary that is either renamed into place or dropped, so a
    /// directory that holds a leftover is a write that took a third path.
    #[test]
    fn a_write_leaves_the_directory_holding_exactly_two_things() {
        let (dir, slots) = slots();
        let mut state = a_run();

        write(&slots, &mut state);
        assert_eq!(entries(dir.path()), BTreeSet::from([SAVE_FILE.to_owned()]));

        write(&slots, &mut state);
        assert_eq!(
            entries(dir.path()),
            BTreeSet::from([SAVE_FILE.to_owned(), BACKUP_FILE.to_owned()])
        );
    }

    /// The rotation, seen from the files: the second write's backup is byte for byte
    /// what the first write left as the primary.
    #[test]
    fn the_second_write_pushes_the_first_into_the_backup() {
        let (_dir, slots) = slots();
        let mut state = a_run();

        write(&slots, &mut state);
        let first = text_at(slots.primary());

        state.tick(Input { space_held: true });
        write(&slots, &mut state);

        assert_eq!(text_at(slots.backup()), first);
        assert_ne!(text_at(slots.primary()), first);
        // Both are still saves, which is the whole point of keeping one.
        assert_eq!(read(slots.backup()).state.last_seen(), NOW);
        assert_eq!(read(slots.primary()).state.last_seen(), NOW);
    }

    #[test]
    fn a_slot_with_no_file_is_not_a_failure() {
        let (_dir, slots) = slots();
        assert!(matches!(load(slots.primary()), Ok(None)));
        assert!(matches!(load(slots.backup()), Ok(None)));
    }

    #[test]
    fn a_written_file_can_say_how_old_it_is_and_a_missing_one_says_nothing() {
        let (_dir, slots) = slots();
        // Nothing there: the recovery screen simply leaves the line out.
        assert!(written_at(slots.primary()).is_none());

        let (mut state, config) = (a_run(), Config::default());
        if let Err(error) = save(&slots, &mut state, &config, NOW) {
            unreachable!("the save should have been written: {error}");
        }
        // The *filesystem's* clock and not `NOW`: `save` writes a run stamped with the
        // instant it was handed, and a modification time the operating system chooses.
        // What the screen needs is that the second one exists at all.
        assert!(written_at(slots.primary()).is_some());
    }

    /// One byte, in the payload — and the backup written moments earlier is untouched,
    /// which is what the recovery screen offers.
    #[test]
    fn one_flipped_byte_in_the_payload_is_refused_and_the_backup_survives() {
        let (_dir, slots) = slots();
        let mut state = a_run();
        write(&slots, &mut state);
        write(&slots, &mut state);

        // Inside the grid, where a byte can flip and leave the document still parsing
        // — so what fails is the signature and not the JSON.
        let text = text_at(slots.primary());
        let at = match text.find("Stone") {
            Some(at) => at,
            None => unreachable!("the fixture run should be in the Stone mine: {text}"),
        };
        let mut bytes = text.into_bytes();
        bytes[at] ^= 1;
        if let Err(error) = fs::write(slots.primary(), bytes) {
            unreachable!("the fixture should be writable: {error}");
        }

        assert!(matches!(load(slots.primary()), Err(PersistError::Tampered)));
        assert_eq!(read(slots.backup()).state.last_seen(), NOW);
    }

    /// One byte, in the signature: a *wrong* mac rather than a malformed one, which is
    /// the difference between [`PersistError::Tampered`] and [`PersistError::Damaged`].
    ///
    /// **The digit is replaced, not bit-flipped**, and the earlier version of this test
    /// did flip it — on the false premise that a hex digit's low bit is another hex
    /// digit. `f ^ 1` is `g`, `a ^ 1` is a backtick, so two starting digits in sixteen
    /// turn the file malformed and the assertion below into its own opposite. Which
    /// digit the mac starts with is a function of the payload, so the test held only
    /// until something moved a byte of the save — the `SAVE_VERSION` bump to 3 is what
    /// finally landed it on `f`. `envelope`'s own `reseat_digit` carries the same
    /// reasoning, and it was wrong there too — passing on the luck of its payload.
    #[test]
    fn one_flipped_byte_in_the_mac_is_refused() {
        let (_dir, slots) = slots();
        write(&slots, &mut a_run());

        let text = text_at(slots.primary());
        let at = match text.find(r#""mac":""#) {
            Some(at) => at + r#""mac":""#.len(),
            None => unreachable!("a written save carries a mac: {text}"),
        };
        let mut bytes = text.into_bytes();
        bytes[at] = if bytes[at] == b'0' { b'1' } else { b'0' };
        if let Err(error) = fs::write(slots.primary(), bytes) {
            unreachable!("the fixture should be writable: {error}");
        }

        assert!(matches!(load(slots.primary()), Err(PersistError::Tampered)));
    }

    /// **The file the atomic write exists to make impossible**, written by hand here
    /// because nothing else can produce one: a save cut in half. It is refused as
    /// damaged rather than half-read, and the backup is still there.
    #[test]
    fn a_file_cut_in_half_is_refused_and_the_backup_survives() {
        let (_dir, slots) = slots();
        let mut state = a_run();
        write(&slots, &mut state);
        write(&slots, &mut state);

        let text = text_at(slots.primary());
        if let Err(error) = fs::write(slots.primary(), &text[..text.len() / 2]) {
            unreachable!("the fixture should be writable: {error}");
        }

        assert!(matches!(load(slots.primary()), Err(PersistError::Damaged)));
        assert_eq!(read(slots.backup()).state.last_seen(), NOW);
    }

    /// When both are gone there is no floor left, and the second recovery frame says
    /// so. Neither file is touched by the refusal — they are still there to look at.
    #[test]
    fn both_files_broken_is_two_refusals_and_no_repair() {
        let (dir, slots) = slots();
        let mut state = a_run();
        write(&slots, &mut state);
        write(&slots, &mut state);

        corrupt(slots.primary());
        corrupt(slots.backup());

        assert!(matches!(load(slots.primary()), Err(PersistError::Tampered)));
        assert!(matches!(load(slots.backup()), Err(PersistError::Tampered)));
        assert_eq!(
            entries(dir.path()),
            BTreeSet::from([SAVE_FILE.to_owned(), BACKUP_FILE.to_owned()]),
            "a refusal must not delete or rotate anything"
        );
    }

    /// Correctly signed, and from a build this one cannot read. It gets its own answer
    /// because it is the one failure the backup cannot help with: a backup written by
    /// that newer build is from the future too.
    #[test]
    fn a_save_from_a_newer_build_is_named_rather_than_called_damaged() {
        let (_dir, slots) = slots();
        write(&slots, &mut a_run());
        reseal(slots.primary(), |payload| {
            payload.replacen(
                &format!(r#""version":{SAVE_VERSION}"#),
                &format!(r#""version":{}"#, SAVE_VERSION + 1),
                1,
            )
        });

        match load(slots.primary()) {
            Err(PersistError::FromTheFuture { found, current }) => {
                assert_eq!(found, u64::from(SAVE_VERSION) + 1);
                assert_eq!(current, SAVE_VERSION);
            }
            other => unreachable!("a newer save must be named as such: {other:?}"),
        }
    }

    /// **The second layer, end to end.** The signature is perfect — this fixture holds
    /// the key, which is precisely the tamperer `docs/SYSTEMS.md` says validation is
    /// for — and the run described is still one the rules could not produce.
    #[test]
    fn a_correctly_signed_but_impossible_run_is_still_refused() {
        let (_dir, slots) = slots();
        write(&slots, &mut a_run());
        reseal(slots.primary(), |payload| {
            payload.replacen(r#""richness_setting":0"#, r#""richness_setting":3"#, 1)
        });

        match load(slots.primary()) {
            Err(PersistError::Rejected(SaveError::Invalid { invariant })) => {
                assert!(
                    invariant.contains("dial"),
                    "unhelpful diagnosis: {invariant}"
                );
            }
            other => unreachable!("an impossible run must be refused: {other:?}"),
        }
    }

    /// A signed payload that is not JSON at all reaches the core as a parse failure,
    /// which is a different sentence from "the file is damaged" even though the screen
    /// is the same.
    #[test]
    fn a_signed_payload_that_is_not_a_run_is_refused_by_the_core() {
        let (_dir, slots) = slots();
        if let Err(error) = fs::create_dir_all(slots.primary().parent().unwrap_or(&slots.dir)) {
            unreachable!("the directory should exist: {error}");
        }
        if let Err(error) = fs::write(slots.primary(), envelope::seal("{}")) {
            unreachable!("the fixture should be writable: {error}");
        }

        assert!(matches!(
            load(slots.primary()),
            Err(PersistError::Rejected(SaveError::MissingVersion))
        ));
    }

    /// A slot whose parent is an ordinary file, which needs neither a permission
    /// change nor root to arrange — but which **the two platforms do not agree is the
    /// same thing**, so this test records the disagreement rather than picking a side.
    ///
    /// Unix separates *"this path is nonsense"* (`ENOTDIR`) from *"the file is not
    /// there"*, and only the second is [`ErrorKind::NotFound`]. Windows reports both
    /// as `ERROR_PATH_NOT_FOUND`, which `std` decodes as `NotFound` — so [`load`]'s
    /// deliberate rule that **a missing file is `Ok(None)`** turns the unreachable
    /// slot into an empty one there. That rule is not the bug; a fresh install really
    /// is the ordinary opening of this game, and the loader has no way to tell the
    /// two apart because the operating system did not.
    ///
    /// Writing refuses on both, since `create_dir_all` meets a file where it needs a
    /// directory. And **the backup answers nothing** either way — it sits behind the
    /// same broken parent as the primary.
    ///
    /// [`ErrorKind::NotFound`]: io::ErrorKind::NotFound
    #[test]
    fn a_location_that_cannot_be_reached_fails_both_ways() {
        let (dir, _) = slots();
        let blocker = dir.path().join("a-file-not-a-directory");
        if let Err(error) = fs::write(&blocker, "in the way") {
            unreachable!("the fixture should be writable: {error}");
        }
        let slots = SaveSlots::in_dir(&blocker.join("skylode"));

        #[cfg(unix)]
        {
            assert!(matches!(load(slots.primary()), Err(PersistError::Io(_))));
            assert!(matches!(load(slots.backup()), Err(PersistError::Io(_))));
        }
        #[cfg(windows)]
        {
            assert!(matches!(load(slots.primary()), Ok(None)));
            assert!(matches!(load(slots.backup()), Ok(None)));
        }

        assert!(matches!(
            save(&slots, &mut a_run(), &Config::default(), NOW),
            Err(PersistError::Io(_))
        ));
    }

    /// **A write that fails leaves the previous save loadable**, which is the property
    /// atomicity is actually for. The failure is forced by taking the write bit off
    /// the directory *after* a good save, so the temporary file cannot be created and
    /// nothing past step 2 runs.
    ///
    /// Skipped where a mode is only a suggestion — running as root, where the write
    /// succeeds regardless. Saying so beats a test that quietly asserts nothing.
    #[cfg(unix)]
    #[test]
    fn a_failed_write_leaves_the_previous_save_intact() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, slots) = slots();
        let mut state = a_run();
        write(&slots, &mut state);
        let before = text_at(slots.primary());

        if let Err(error) = fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)) {
            unreachable!("the fixture should be chmod-able: {error}");
        }
        let denied = fs::write(dir.path().join("probe"), "").is_err();
        let outcome = save(&slots, &mut state, &Config::default(), NOW);
        // Put the bit back before any assertion, or a failure leaves a directory the
        // harness cannot clean up.
        if let Err(error) = fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)) {
            unreachable!("the fixture should be chmod-able: {error}");
        }

        if denied {
            assert!(matches!(outcome, Err(PersistError::Io(_))), "{outcome:?}");
            assert_eq!(text_at(slots.primary()), before);
            assert_eq!(read(slots.primary()).state.last_seen(), NOW);
        }
    }

    /// Every refusal is renderable: the recovery screen shows one of these, and a
    /// message that says nothing is a screen that helps nobody. The two that wrap
    /// another error also forward it as their [`source`](std::error::Error::source),
    /// which is what lets a front-end log the detail under the friendly line.
    #[test]
    fn every_refusal_says_something() {
        use std::error::Error;

        let refusals = [
            PersistError::Io(io::Error::from(io::ErrorKind::PermissionDenied)),
            PersistError::Damaged,
            PersistError::Tampered,
            PersistError::FromTheFuture {
                found: 9,
                current: SAVE_VERSION,
            },
            PersistError::Rejected(SaveError::MissingVersion),
        ];
        for refusal in &refusals {
            assert!(!refusal.to_string().is_empty(), "{refusal:?} says nothing");
        }

        assert!(refusals[0].source().is_some());
        assert!(refusals[4].source().is_some());
        assert!(refusals[1].source().is_none());
        assert!(refusals[2].source().is_none());
        assert!(refusals[3].source().is_none());
    }
}
