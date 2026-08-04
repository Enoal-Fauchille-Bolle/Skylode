//! The session: who owns the terminal, and what the player is looking at.
//!
//! [`App`] is *the game* — a run, the screens over it, the toasts. A session is the
//! thing above it: the loop that draws, waits and steps, plus the three states that
//! are **not** a game and could not be represented by an `App` without making
//! `state: GameState` an [`Option`] that fifty-odd call sites would then have to
//! unwrap. The title screen has no run. That is the whole argument for this file.
//!
//! ## The machine, and where its edges come from
//!
//! `docs/UI.md` §8.3 draws it. Three properties of that drawing are decisions rather
//! than arrangement, and each one is a branch below:
//!
//! - **Recovery runs before the title**, so the player whose save will not load never
//!   meets a menu offering to continue it.
//! - **`Continue` exists only on paths that reached a trusted save.** It is spelled
//!   here as [`Splash::resume`] being [`Some`]: there is no flag to forget to clear.
//! - **A missing save is a fresh install only when the backup is missing too.** The
//!   atomic write is two renames, so there is an instant in which the backup exists
//!   and the save does not, and a crash exactly there must not walk a player past
//!   their own run to a `New game`.
//!
//! ## `Continue` re-reads the file, and that is deliberate
//!
//! The title holds no run in memory — only a [`Resume`] summary and the *path* it
//! came from. So `Continue` loads from the disk on every path, whether the player
//! just launched the game or just walked out of one. It costs a read of a few
//! kilobytes and it buys two things: `Continue` means exactly one thing, and the
//! save's round trip is exercised in real play rather than only in tests.
//!
//! ## The redraw policy lives here
//!
//! *When* to ask the terminal for a frame is a question about the session — which
//! state is up, whether anything moved — while *what changed* is the run's answer,
//! and [`App::advance`] returns it rather than writing into a flag it does not own.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use color_eyre::Result;
use ratatui::{Frame, Terminal, backend::Backend, crossterm::event::KeyEvent, layout::Rect};
use skylode_core::{
    enchant::EnchantType,
    game::{GameState, OfflineReport},
    save::Save,
};

use crate::{
    action::{Action, MenuAction},
    app::{App, Leaving},
    config::Config,
    event::{Event, Events},
    format::rung_label,
    keymap,
    overlay::{offline, save_recovery, splash, too_small},
    persist::{self, PersistError, SaveSlots},
    toast::{TOAST_TTL, Tone},
};

/// The shortest gap between two draws — a **ceiling on the redraw rate**, not a
/// cadence.
///
/// The loop draws when something changed *and* this much time has passed, so a burst
/// of held keys cannot ask the terminal for two hundred frames a second. It is
/// deliberately shorter than the simulation period: today the simulation is the only
/// thing that changes the screen, so the real rate is 20 fps and this ceiling never
/// binds — but the proc flash (two stages of ~100 ms, `docs/UI.md` §7) changes the
/// screen *between* ticks, and it is the reason the two clocks are separate now
/// rather than separated later.
/// **Input is exempt**: a key that meant something draws on the spot, because the
/// only burst it can produce is bounded by the terminal's own repeat rate, and 33 ms
/// of latency in the one place the player is looking is worse than a frame nobody
/// asked for.
const FRAME_PERIOD: Duration = Duration::from_millis(33);

/// How often a running game is written to disk (`docs/SYSTEMS.md` §*Save cadence*).
///
/// **Unconditional, and the `dirty` flag that section asks for is deliberately not
/// here.** That flag was specified before the auto-miner existed; today every single
/// tick credits it, so "has the state changed since the last write" is a `bool` that
/// cannot be false while a game is up — and a field that cannot be false is a field
/// that lies about what it is for. The saving it was meant to buy is instead
/// structural: the title and the recovery frames have no run to write, so the loop
/// there does not reach this clock at all.
///
/// Ten seconds is what bounds the loss: a crash costs at most that much mining, on
/// top of which every important transaction writes on the spot.
const AUTOSAVE_PERIOD: Duration = Duration::from_secs(10);

/// The seed a fresh run starts from, taken from the wall clock.
///
/// **This is the only entropy in the game, and it is deliberately on this side of
/// the crate boundary.** `skylode-core` compiles `rand` with
/// `default-features = false`, which strips `thread_rng` and `os_rng` out of the
/// build entirely — so the determinism contract is enforced by the compiler rather
/// than by discipline, and a seed *has* to be handed in from outside.
///
/// **It takes `now` rather than reading the clock**, which is what moved it out of
/// `main`. A new run can begin at any moment the player picks `New game` on the
/// title, long after `main` has handed the loop over; and the session already holds
/// a wall-clock reading at every transition, so passing it costs nothing and keeps
/// the number testable.
///
/// Nanoseconds since the epoch, not seconds: two runs started in the same second
/// should not lay out the same mine. A clock before 1970 falls back to `0`, which is
/// a legal seed — a wrong clock should give a boring run, not no run.
pub fn seed_from(now: SystemTime) -> u64 {
    now.duration_since(UNIX_EPOCH)
        .map_or(0, |since_epoch| since_epoch.as_nanos() as u64)
}

/// Whether this gesture is worth a write of its own, ahead of the clock.
///
/// `docs/SYSTEMS.md` §*Save cadence* asks for one *"on important transactions"*, and
/// these three are what that means here: [`Confirm`](Action::Confirm) takes every
/// modal — a purchase, a compression, a prestige — while
/// [`BuyMax`](Action::BuyMax) and [`ClaimAll`](Action::ClaimAll) are the two gestures
/// that spend or collect without one.
///
/// **The list is short on purpose.** Everything else — walking a cursor, sliding the
/// richness dial, changing tab — is either free to redo or caught by the ten-second
/// clock, and a session that wrote on every keystroke would be a session that wrote
/// on the arrow keys.
fn banks(action: &Action) -> bool {
    matches!(action, Action::Confirm | Action::BuyMax | Action::ClaimAll)
}

/// A running session: one state, the loop that drives it, and where it saves.
#[derive(Debug)]
pub struct Session {
    /// The two files this session reads and writes, or [`None`] when the platform
    /// would not name a directory for them.
    ///
    /// **An [`Option`] at the edge and not a null backend**: without a [`SaveSlots`]
    /// there is no way to ask `persist` to write, so "this session cannot save" is a
    /// shape the compiler enforces rather than a flag someone must remember to test.
    /// The player is told twice — a line on the title, because it explains the
    /// `Continue` that is not there, and a toast on entering a game, because that is
    /// where the consequence starts.
    slots: Option<SaveSlots>,
    /// What the player is looking at.
    stage: Stage,
    /// The earliest the next draw may happen — [`FRAME_PERIOD`]'s ceiling.
    ///
    /// **A deadline, not a countdown**, the same shape the simulation's clock uses
    /// inside [`App`]: it is compared against the wall clock and reset from it, so a
    /// late pass does not push every later frame back by what it overshot.
    next_frame: Instant,
    /// Whether anything has changed since the last draw.
    ///
    /// *Redraw on change* in the one form the front-end can answer cheaply: raised by
    /// a key that was acted on, a resize, and any simulation step that ran. On the
    /// title and the recovery frames nothing ages, so the loop there is genuinely
    /// idle: the heartbeat wakes it, it finds nothing to do, and it asks the terminal
    /// for nothing at all.
    dirty: bool,
    /// Whether the last frame found the terminal below the 80×24 budget.
    ///
    /// Kept rather than asked at the keystroke, because the loop **draws before it
    /// waits**: a resize raises `dirty`, the next pass redraws, and only then is a key
    /// read. So this is never stale when it is consulted.
    cramped: bool,
    /// When the running game is next due to be written — [`AUTOSAVE_PERIOD`]'s clock.
    ///
    /// The **fourth** deadline in this loop, beside the frame ceiling here and the
    /// simulation's inside [`App`], and it is held the same way: compared against the
    /// wall clock and reset from it, so a busy frame does not push the next write back
    /// by what it overshot. It only matters while a [`Stage::Game`] is up, and it is
    /// reset by [`open_game`](Session::open_game) — which has just written — so the
    /// first autosave of a run falls a full period after the run opened rather than
    /// immediately.
    next_autosave: Instant,
    /// Whether the last write failed.
    ///
    /// **A `bool` whose whole job is to make the toast an *edge*.** A full disk fails
    /// every ten seconds, and announcing each one would bury the game under identical
    /// refusals; announcing only the transitions means the player hears once that
    /// saving broke and once that it works again. It is deliberately **not fatal** —
    /// the run in memory is fine, and throwing it away would be the opposite of what
    /// *"no continue anyway"* protects.
    save_failing: bool,
    /// Set when the process should end.
    quit: bool,
    /// Whether every game this session opens gets the dev menu.
    ///
    /// Held here and not read from the environment again: `main` does the reading, and
    /// a session can open several games (a `New game` after a recovery, and every
    /// `Continue`), each of which has to be built the same way.
    #[cfg(debug_assertions)]
    dev: bool,
}

/// What the player is looking at — `docs/UI.md` §8.3's nodes, minus the checks.
///
/// The diagram's *"HMAC check"* and *"Backup HMAC check"* are not states here: they
/// are questions [`persist::load`] answers in one call, and a state the loop can
/// linger in would be a state that has to draw something.
#[derive(Debug)]
enum Stage {
    /// The title (§6.1).
    Splash(Splash),
    /// A save that would not load, and what can still be done about it (§6.3).
    Recovery(Recovery),
    /// A run the player has not looked at yet, and what their absence paid (§6.4).
    ///
    /// **The run is already credited and already written** by the time this is built:
    /// `resume` moved the mark and added the ore, and the summary is a *reading* of
    /// what it did. Pressing `Enter` collects nothing — it dismisses a receipt.
    Offline {
        /// The run behind the modal, drawn under it for context.
        app: Box<App>,
        /// What the absence paid.
        report: OfflineReport,
    },
    /// A run.
    ///
    /// [`Box`]ed because an enum is as large as its largest variant, and [`App`] is a
    /// few hundred bytes of screens, cursors and toasts against the dozen a title
    /// screen needs. The box costs one pointer hop per frame and keeps a `Stage` small
    /// enough to move around freely.
    Game(Box<App>),
}

/// The title screen's state (§6.1).
#[derive(Debug)]
pub struct Splash {
    /// The run on disk this title can continue, or [`None`] when there is none.
    ///
    /// **This *is* "`Continue` exists only on paths that reached a trusted save".**
    /// Every route into the title either carries a save it has just loaded
    /// successfully or carries nothing, so the menu cannot offer a row that leads
    /// somewhere unreadable.
    resume: Option<Resume>,
    /// Which of [`rows`](Splash::rows) the caret is on.
    cursor: usize,
    /// Whether this session can write at all — the line under the menu.
    persists: bool,
    /// Which row of the *"start a new game?"* box the caret is on, while it is up.
    ///
    /// **One [`Option`] and not a `bool` beside a cursor**, so "the box is closed" and
    /// "the box is open on its second row" cannot be held at once. The box exists only
    /// where it has something to protect: §6.1 draws no confirmation, and this is a
    /// departure taken because `New game` sits one arrow key away from `Continue` and
    /// the first autosave ten seconds later writes over the run it was next to.
    confirm: Option<usize>,
}

/// What a title screen knows about the save it is offering to continue.
///
/// **It carries the path it came from**, which is not decoration. A save loaded from
/// the *backup* — the primary was missing, the window between the atomic write's two
/// renames — must be re-read from the backup when the player presses `Continue`;
/// re-reading the primary would find nothing there and walk them onto a fresh install
/// over the top of their own run.
#[derive(Debug)]
pub struct Resume {
    /// The file `Continue` re-reads.
    source: PathBuf,
    /// The mining level, for `Lv 23`.
    level: u32,
    /// The pickaxe rung, for `Diamond Pickaxe`.
    pickaxe: String,
    /// How long ago the run was last written, when the clock allows the subtraction.
    ///
    /// [`None`] on a backward clock, which is the same clamp
    /// [`GameState::resume`](skylode_core::game::GameState::resume) makes: a DST
    /// change is not a fact worth printing a negative about.
    idle: Option<Duration>,
    /// Whether this came from the backup rather than from the save proper.
    from_backup: bool,
}

/// A row of the *"start a new game?"* box.
///
/// **Two rows and the safe one first.** The caret opens on `Keep`, so the gesture that
/// destroys a run is never the one a stray `Enter` lands on — which is exactly the
/// accident the box exists for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmRow {
    /// Back to the menu, run untouched.
    Keep,
    /// Start over, and let the next write have the slot.
    StartOver,
}

/// The rows the confirmation box offers, in order.
pub const CONFIRM_ROWS: [ConfirmRow; 2] = [ConfirmRow::Keep, ConfirmRow::StartOver];

/// A row of the title's menu.
///
/// `Settings` is **not** here yet: it is phase 9's screen, and a row that highlights
/// and does nothing teaches the player that the caret is decorative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplashRow {
    /// Load the save this title was built from.
    Continue,
    /// Start over.
    NewGame,
    /// Leave.
    Quit,
}

/// The recovery screen's state (§6.3).
#[derive(Debug)]
pub struct Recovery {
    /// What went wrong, which decides both the sentence and the rows.
    trouble: Trouble,
    /// Which of [`rows`](Recovery::rows) the caret is on.
    cursor: usize,
}

/// What the loader said, in the shape the *screens* need.
///
/// **Four causes, three frames**, and the grouping is the point: two causes share a
/// variant exactly when they share a destination, and two variants share a frame when
/// they differ only in their first sentence. [`PersistError`] is the loader's
/// vocabulary; this is the player's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trouble {
    /// The save failed its check, and the backup has not been tried yet.
    ///
    /// `age` is how old the backup was when the game looked, which is the number the
    /// player's decision is actually about. It is [`None`] when there is no backup —
    /// and the row is still offered, because §8.3 makes *trying* it the player's move:
    /// choosing it then lands on [`NothingLeft`](Trouble::NothingLeft), which is the
    /// diagram's `BakMac -> RecNoBak: backup bad or absent` edge.
    BackupOffered {
        /// How long ago the backup was written, when that can be asked.
        age: Option<Duration>,
    },
    /// The save failed its check and so did the backup: no floor left.
    NothingLeft,
    /// The bytes could not be reached at all — a permission, a vanished disk.
    ///
    /// A frame of its own sentence but not of its own shape, because the answer is
    /// [`NothingLeft`](Trouble::NothingLeft)'s: both files share a directory, so
    /// whatever stopped one stops the other.
    Unreadable,
    /// A newer build wrote it.
    FromTheFuture {
        /// The version the file claims.
        found: u64,
        /// The newest version this build understands.
        current: u32,
    },
}

/// A row of a recovery frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryRow {
    /// Try the `.bak` — §8.3's `Rec -> BakMac` edge, and a keypress rather than
    /// something the loader did on its own.
    RestoreBackup,
    /// Abandon the file and start over.
    NewGame,
    /// Leave, changing nothing.
    Quit,
}

/// What a `Confirm` on a menu screen resolves to.
///
/// Lifted out of the [`Stage`] before it is acted on, because acting needs `&mut
/// self` and reading the row needs `&self`: the row is [`Copy`], so taking it out
/// first is what lets the two happen in one function without fighting the borrow
/// checker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Chosen {
    /// The title's `Continue`.
    Resume,
    /// A fresh run, from either screen.
    NewGame,
    /// The recovery frame's `Restore the backup`.
    RestoreBackup,
    /// The offline summary's `Enter` — dismiss the receipt and walk into the run.
    Collect,
    /// `New game` where there is a run to lose: raise the box rather than act.
    AskNewGame,
    /// The box's `Keep`, or an `Esc` over it.
    Cancel,
    /// Leave.
    Quit,
}

impl Splash {
    /// A title with nothing to continue.
    fn fresh(persists: bool) -> Self {
        Self {
            resume: None,
            cursor: 0,
            persists,
            confirm: None,
        }
    }

    /// A title over a save that has just loaded.
    fn over(resume: Resume, persists: bool) -> Self {
        Self {
            resume: Some(resume),
            cursor: 0,
            persists,
            confirm: None,
        }
    }

    /// The rows this title offers, in order.
    ///
    /// A `&'static` slice and not a [`Vec`]: the menu is one of exactly two lists, so
    /// building one per keystroke would be an allocation to answer a question with two
    /// answers.
    pub fn rows(&self) -> &'static [SplashRow] {
        if self.resume.is_some() {
            &[SplashRow::Continue, SplashRow::NewGame, SplashRow::Quit]
        } else {
            &[SplashRow::NewGame, SplashRow::Quit]
        }
    }

    /// Which row the caret is on.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// What the summary lines describe, if there is anything to continue.
    pub fn resume(&self) -> Option<&Resume> {
        self.resume.as_ref()
    }

    /// Whether this session can write a save at all.
    pub fn persists(&self) -> bool {
        self.persists
    }

    /// Which row of the confirmation box the caret is on, while the box is up.
    pub fn confirm(&self) -> Option<usize> {
        self.confirm
    }

    /// A title screen with made-up contents, for the renderer's own tests.
    ///
    /// **The same device as [`View::sample`](crate::view::View::sample)**, and it
    /// exists for the same reason: `overlay::splash` is tested on what it *draws*, and
    /// reaching a given title through [`Session::boot`] would mean writing a save to a
    /// temporary directory to assert the position of a caret. The states the machine
    /// can actually reach are asserted in `session`'s own tests, where the boot routing
    /// is what is under test.
    /// The same, with the *"start a new game?"* box up over it.
    #[cfg(test)]
    pub(crate) fn sample_confirming() -> Self {
        Self {
            confirm: Some(0),
            ..Self::sample(true, true)
        }
    }

    #[cfg(test)]
    pub(crate) fn sample(persists: bool, over_a_save: bool) -> Self {
        let resume = over_a_save.then(|| Resume {
            source: PathBuf::from("save.json"),
            level: 23,
            pickaxe: "Diamond Pickaxe".to_owned(),
            idle: Some(Duration::from_secs(3 * 60 * 60)),
            from_backup: false,
        });
        Self {
            resume,
            cursor: 0,
            persists,
            confirm: None,
        }
    }
}

impl Resume {
    /// What a title screen should say about `save`, read at `now`.
    fn of(save: &Save<Config>, source: &Path, now: SystemTime, from_backup: bool) -> Self {
        let player = save.state.player();
        let pickaxe = player.get_pickaxe();
        Self {
            source: source.to_path_buf(),
            level: player.get_level(),
            // The same label the Upgrades ladder and the purchase toast use, rather
            // than a second way of naming a pickaxe: a title screen that said
            // `Diamond` where the game says `Diamond Eff V` would be describing a
            // different rung.
            pickaxe: rung_label(
                pickaxe.get_tier(),
                pickaxe.enchants().get_level(EnchantType::Efficiency),
            ),
            idle: now.duration_since(save.state.last_seen()).ok(),
            from_backup,
        }
    }

    /// The mining level to print.
    pub fn level(&self) -> u32 {
        self.level
    }

    /// The pickaxe rung to print.
    pub fn pickaxe(&self) -> &str {
        &self.pickaxe
    }

    /// How long the run has been sitting, when that can be said.
    pub fn idle(&self) -> Option<Duration> {
        self.idle
    }
}

impl Recovery {
    /// A recovery screen over `trouble`, caret on its first row.
    fn new(trouble: Trouble) -> Self {
        Self { trouble, cursor: 0 }
    }

    /// What went wrong.
    pub fn trouble(&self) -> Trouble {
        self.trouble
    }

    /// The rows this frame offers.
    ///
    /// **A save from the future offers only `Quit`**, and that is a departure from
    /// §8.3's `RecNoBak -> new game` edge, taken deliberately: the file is *good*. It
    /// failed for being newer than this build, so starting a run over it would let an
    /// older build overwrite a save the player made with a newer one — the one refusal
    /// in the table where "start again" destroys something that was never broken.
    pub fn rows(&self) -> &'static [RecoveryRow] {
        match self.trouble {
            Trouble::BackupOffered { .. } => &[
                RecoveryRow::RestoreBackup,
                RecoveryRow::NewGame,
                RecoveryRow::Quit,
            ],
            Trouble::NothingLeft | Trouble::Unreadable => {
                &[RecoveryRow::NewGame, RecoveryRow::Quit]
            }
            Trouble::FromTheFuture { .. } => &[RecoveryRow::Quit],
        }
    }

    /// Which row the caret is on.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// A recovery screen over `trouble`, for the renderer's own tests.
    ///
    /// [`Splash::sample`]'s reason exactly: reaching `FromTheFuture` through the boot
    /// routing costs a re-signed fixture on disk, and what this asserts is the frame.
    #[cfg(test)]
    pub(crate) fn sample(trouble: Trouble) -> Self {
        Self::new(trouble)
    }
}

impl Session {
    /// Opens a session by looking at what is on the disk (§8.3).
    ///
    /// **Nothing is repaired and nothing is deleted here either.** `persist` reads one
    /// slot and reports; this routes the report to a screen. The two halves are kept
    /// apart so that *"restore the backup"* stays a keypress the player makes rather
    /// than a fallback a loader takes on their behalf.
    pub fn boot(slots: Option<SaveSlots>, now: SystemTime) -> Self {
        let stage = Self::look(slots.as_ref(), now);
        Self {
            slots,
            stage,
            next_frame: Instant::now(),
            dirty: true,
            cramped: false,
            next_autosave: Instant::now() + AUTOSAVE_PERIOD,
            save_failing: false,
            quit: false,
            #[cfg(debug_assertions)]
            dev: false,
        }
    }

    /// Turns the dev menu on for every game this session opens.
    ///
    /// **A builder step and not a parameter of [`boot`](Session::boot)**, matching
    /// [`App::with_dev`], so that the tests that boot a session say nothing about a
    /// feature they are not about. `main` is the only non-test caller.
    #[cfg(debug_assertions)]
    pub fn with_dev(mut self, enabled: bool) -> Self {
        self.dev = enabled;
        self
    }

    /// Where a launch — or a return to the title — lands, given what is on the disk.
    ///
    /// One function and one caller-visible rule, which is what makes `q` cheap later:
    /// leaving a game re-runs exactly this, so "what does `Continue` mean" has one
    /// answer no matter how the player got to the title.
    fn look(slots: Option<&SaveSlots>, now: SystemTime) -> Stage {
        let Some(slots) = slots else {
            return Stage::Splash(Splash::fresh(false));
        };
        match persist::load(slots.primary()) {
            Ok(Some(save)) => Stage::Splash(Splash::over(
                Resume::of(&save, slots.primary(), now, false),
                true,
            )),
            // **Not a fresh install on its own.** The atomic write is two renames, and
            // between them the backup exists while the save does not; a crash exactly
            // there must not walk the player past a perfectly good run.
            Ok(None) => Self::from_backup(slots, now),
            // The backup answers neither of these. `Io` because both files share a
            // directory, so whatever stopped one stops the other; `FromTheFuture`
            // because a backup written by that same newer build is from the future too.
            Err(PersistError::Io(_)) => Stage::Recovery(Recovery::new(Trouble::Unreadable)),
            Err(PersistError::FromTheFuture { found, current }) => {
                Stage::Recovery(Recovery::new(Trouble::FromTheFuture { found, current }))
            }
            Err(_) => Stage::Recovery(Recovery::new(Trouble::BackupOffered {
                age: Self::age_of(slots.backup(), now),
            })),
        }
    }

    /// §8.3's `BakMac` node: what the backup turns out to be.
    ///
    /// **A good backup lands on the title, not in a game**, which is the diagram's own
    /// edge and not a shortcut skipped: `Continue` is offered only from a trusted save,
    /// and arriving at the title is what proves this one is trusted. It also shows the
    /// player what they are about to resume before they resume it.
    fn from_backup(slots: &SaveSlots, now: SystemTime) -> Stage {
        match persist::load(slots.backup()) {
            Ok(Some(save)) => Stage::Splash(Splash::over(
                Resume::of(&save, slots.backup(), now, true),
                true,
            )),
            Ok(None) => Stage::Splash(Splash::fresh(true)),
            Err(PersistError::Io(_)) => Stage::Recovery(Recovery::new(Trouble::Unreadable)),
            Err(_) => Stage::Recovery(Recovery::new(Trouble::NothingLeft)),
        }
    }

    /// How long ago the file at `path` was written, when both clocks allow it.
    fn age_of(path: &Path, now: SystemTime) -> Option<Duration> {
        now.duration_since(persist::written_at(path)?).ok()
    }

    /// Draws, then blocks for the next event, until asked to quit.
    ///
    /// Rendering happens *before* waiting so the first frame appears immediately
    /// rather than after the first keypress. Blocking on `events.next()` is what
    /// keeps the app at zero CPU while idle; the tick guarantees we still wake up
    /// often enough to expire toasts.
    ///
    /// **Generic over both of its collaborators, so that the loop itself can be
    /// tested.** It used to take a `DefaultTerminal` — ratatui's alias for
    /// `Terminal<CrosstermBackend<Stdout>>` — and a concrete
    /// [`EventHandler`](crate::event::EventHandler), and between them they made this
    /// function unreachable from a test: the backend writes to the real stdout, and
    /// the handler's thread dies the moment it polls a terminal that is not there.
    /// Everything else in the crate is exercised through ratatui's own `TestBackend`;
    /// these two parameters are what let the loop join it.
    ///
    /// Generics rather than `dyn`: both types are known at every call site, so the
    /// compiler emits one specialised copy per pair (*monomorphisation*) and the
    /// indirection costs nothing at runtime. `main` still passes the real terminal
    /// and the real handler, and neither has to change.
    ///
    /// The `where` clause is what `?` needs. Since ratatui 0.30 a backend names its
    /// own error type rather than always being `io::Error`, and `color_eyre::Report`
    /// can only absorb one that is a `std::error::Error` it can carry across threads
    /// and outlive the frame. Both real backends satisfy it, including
    /// `TestBackend`, whose error is [`Infallible`](core::convert::Infallible) — a
    /// type with no values, so the conversion is one that provably never runs.
    /// The terminal is borrowed, not consumed: `run` has no business dropping it —
    /// `main` restores the screen afterwards and needs it alive to do so — and a
    /// caller that could not look at the terminal again after the loop returned could
    /// not read the last frame the player saw.
    pub fn run<B, E>(mut self, terminal: &mut Terminal<B>, events: E) -> Result<()>
    where
        B: Backend,
        B::Error: std::error::Error + Send + Sync + 'static,
        E: Events,
    {
        loop {
            let now = Instant::now();
            // Before the wait, not after: the first frame must show the state as it
            // stands rather than appearing on the first keypress. `boot` starts both
            // flags due, so the opening pass always draws.
            if self.dirty && now >= self.next_frame {
                let size = terminal.size()?;
                self.cramped = !too_small::fits(Rect::new(0, 0, size.width, size.height));
                self.sync_view(now);
                terminal.draw(|frame| self.render(frame, now))?;
                self.dirty = false;
                self.next_frame = now + FRAME_PERIOD;
            }

            // Not a bare `?`: a dead event source ends the session, and the run in
            // memory is worth more than the ten seconds the cadence would have owed
            // it. A real `EventHandler` gets here when its thread has died and the
            // channel closed, which is not a reason to lose a swing.
            let event = match events.next() {
                Ok(event) => event,
                Err(error) => {
                    self.autosave(SystemTime::now());
                    return Err(error);
                }
            };

            match event {
                // **Nothing.** The heartbeat's whole job is to end the block above's
                // wait so that `advance` below gets to look at the clock; how many
                // beats arrived is not a quantity anything here counts. That is the
                // difference between a heartbeat and a cadence, and it is what lets
                // the simulation keep 20 tps whatever rate this channel runs at.
                Event::Tick => {}
                Event::Key(key) => {
                    if self.on_key(key) {
                        // Only a key that *meant* something redraws. An unbound key
                        // changed nothing, and a frame that redraws the same buffer
                        // is work the terminal has to undo.
                        self.dirty = true;
                        // **And it redraws now, not at the next allowed frame.** The
                        // ceiling exists to stop the *simulation* from asking for two
                        // hundred frames a second; input cannot ask for more than the
                        // keyboard repeats, and a tab that appears 33 ms after the key
                        // is latency the player feels in the one place they are
                        // looking. Bursts stay bounded because a terminal's repeat
                        // rate is.
                        self.next_frame = now;
                    }
                }
                // ratatui lays out against the new size on the next draw; all this
                // has to do is make sure there is one.
                Event::Resize => self.dirty = true,
            }

            if self.quit {
                break;
            }

            // After the event and not before: a key pressed this pass should reach
            // the step it belongs to, not the one after it.
            let now = Instant::now();
            self.dirty |= self.advance(now);

            // And after the step, so what reaches the disk includes it. The stage is
            // checked here rather than inside `autosave` so that a title screen does
            // not push its deadline forward every pass and then write the instant a
            // game opens.
            if matches!(self.stage, Stage::Game(_)) && now >= self.next_autosave {
                self.autosave(SystemTime::now());
                self.next_autosave = now + AUTOSAVE_PERIOD;
            }
        }
        Ok(())
    }

    /// Writes the running game, and announces only a *change* in whether that works.
    ///
    /// **It never calls `touch`.** [`persist::save`] moves
    /// [`last_seen`](skylode_core::game::GameState::last_seen) itself, before
    /// serialising, because a caller that touched afterwards would write the previous
    /// mark and have the next absence measured from a moment already paid for. This
    /// supplies `now` and nothing else.
    ///
    /// Silent on every state that holds no run, and on a session with nowhere to
    /// write: both are ordinary, and neither is news.
    fn autosave(&mut self, now: SystemTime) {
        let Some(slots) = &self.slots else {
            return;
        };
        let Some(app) = Self::running_mut(&mut self.stage) else {
            return;
        };
        // Two disjoint fields of the same `App`, which is why this borrows rather than
        // taking a clone: the run is a few kilobytes and the write happens twice a
        // minute.
        let outcome = persist::save(slots, &mut app.state, &app.config, now);
        match outcome {
            Err(error) if !self.save_failing => {
                app.toasts
                    .push(format!("Save failed: {error}"), Tone::Refusal, TOAST_TTL);
                self.save_failing = true;
            }
            Ok(()) if self.save_failing => {
                app.toasts
                    .push("Saving works again".to_owned(), Tone::Success, TOAST_TTL);
                self.save_failing = false;
            }
            // A second failure in a row, or an ordinary success. The player has already
            // been told which of the two the game is in.
            _ => {}
        }
    }

    /// The run this state holds, if it holds one.
    ///
    /// **An associated function taking the field rather than a method taking `self`**,
    /// so that a caller which has already borrowed another field — [`autosave`] holds
    /// [`slots`](Session#structfield.slots) — can still reach the run. Borrowing
    /// through `&mut self` would take the whole struct and make those two borrows
    /// fight, for no reason other than how the signature was written.
    ///
    /// [`autosave`]: Session::autosave
    fn running_mut(stage: &mut Stage) -> Option<&mut App> {
        match stage {
            Stage::Game(app) | Stage::Offline { app, .. } => Some(app),
            Stage::Splash(_) | Stage::Recovery(_) => None,
        }
    }

    /// Runs whatever the clock owes the current state.
    ///
    /// **Only a game has a clock**, and the offline summary deliberately does not: the
    /// player is reading a receipt for time they have already been paid for, and a run
    /// mining behind the box would make the moment they press `Enter` part of the sum.
    /// The title and the recovery frames are still pictures for a simpler reason —
    /// there is no run at all — so the loop over any of the three is idle by
    /// construction rather than by a paused flag someone has to remember to set.
    ///
    /// **That makes this the first state to pause a tick, and phase 7 left a note due
    /// about it**: the proc flash resolves its beat in `sync_view` rather than in
    /// `render`, so a live flash under a paused tick would freeze mid-beat. It cannot
    /// happen here, and by construction rather than by a `clear`: the only `App` this
    /// state ever holds has just been built from a load, so its `Flashes` is empty. A
    /// future state that pauses a *running* game will have to clear it.
    fn advance(&mut self, now: Instant) -> bool {
        match &mut self.stage {
            Stage::Game(app) => app.advance(now),
            Stage::Offline { .. } | Stage::Splash(_) | Stage::Recovery(_) => false,
        }
    }

    /// Rebuilds the read model, where there is one.
    fn sync_view(&mut self, now: Instant) {
        if let Some(app) = Self::running_mut(&mut self.stage) {
            app.sync_view(now);
        }
    }

    /// Paints one frame of whatever is up.
    ///
    /// **The too-small filter is here and not inside a state** (§6.2). It replaces the
    /// whole frame regardless of which state is up — including the title, which is why
    /// it could not stay in [`App::render`] — and yields it back untouched once the
    /// window grows, because it reads no state at all. Drawing it before anything is
    /// split off the area is what "a filter, not a state with edges" means in code.
    fn render(&self, frame: &mut Frame, now: Instant) {
        let area = frame.area();
        if !too_small::fits(area) {
            too_small::render(frame, area);
            return;
        }
        match &self.stage {
            Stage::Splash(state) => splash::render(frame, area, state),
            Stage::Recovery(state) => save_recovery::render(frame, area, state),
            Stage::Game(app) => app.render(frame, now),
            // The run is drawn *under* the summary, which is what makes it a modal
            // rather than a screen: the player can see the mine they are about to walk
            // back into while they read what it earned without them.
            Stage::Offline { app, report } => {
                app.render(frame, now);
                offline::render(frame, area, report);
            }
        }
    }

    /// Answers one key, saying whether it meant anything.
    fn on_key(&mut self, key: KeyEvent) -> bool {
        // Above the states, for the reason the filter is above them at the draw: while
        // the window is too small there is one affordance on screen and it says *quit*.
        if self.cramped {
            if keymap::resolve_too_small(key).is_some() {
                self.quit = true;
                return true;
            }
            return false;
        }

        match &mut self.stage {
            Stage::Game(app) => {
                let Some(action) = keymap::resolve(app, key) else {
                    return false;
                };
                // Asked before the reducer runs, because `Action` is `Clone` and not
                // `Copy` — the prestige confirm carries a `String` — so `update` takes
                // ownership and there is nothing left to ask afterwards.
                let banked = banks(&action);
                app.update(action);
                let leaving = app.leaving;
                // The borrow of `self.stage` above ends at the last use of `app`, which
                // is what lets the rest of this arm reach back into `self`.
                let now = SystemTime::now();

                // **The write comes first on every exit**, and on the `ToTitle` path
                // that ordering is load-bearing rather than tidy: the title is rebuilt
                // by re-reading the file, so a save that happened afterwards would
                // build a title out of the *previous* run — `Continue` would offer the
                // state the player had ten seconds ago.
                if leaving.is_some() || banked {
                    self.autosave(now);
                }
                match leaving {
                    Some(Leaving::Process) => self.quit = true,
                    Some(Leaving::ToTitle) => self.stage = Self::look(self.slots.as_ref(), now),
                    None => {}
                }
                true
            }
            Stage::Splash(_) | Stage::Recovery(_) | Stage::Offline { .. } => {
                let Some(action) = keymap::resolve_menu(key) else {
                    return false;
                };
                self.menu(action);
                true
            }
        }
    }

    /// Applies one menu gesture to whichever list is up.
    fn menu(&mut self, action: MenuAction) {
        match action {
            MenuAction::Up => self.move_caret(-1),
            MenuAction::Down => self.move_caret(1),
            MenuAction::Confirm => self.confirm(SystemTime::now()),
            MenuAction::Cancel => self.cancel(),
            MenuAction::Quit => self.quit = true,
        }
    }

    /// Walks the caret, wrapping at both ends.
    ///
    /// The wrap is the list's own length rather than a number written down, which is
    /// what keeps it right when the title loses `Continue` or a recovery frame offers
    /// one row instead of three. `rem_euclid` is what makes a step off the top come
    /// out at the bottom: the plain `%` in Rust keeps the sign of the left operand, so
    /// `-1 % 3` is `-1` and not `2`.
    fn move_caret(&mut self, by: isize) {
        let (cursor, len) = match &self.stage {
            // The box owns the caret while it is up, which is what keeps one gesture
            // driving both lists: `Up` is `Up` whether the player is choosing a menu
            // row or answering a question.
            Stage::Splash(splash) => match splash.confirm {
                Some(cursor) => (cursor, CONFIRM_ROWS.len()),
                None => (splash.cursor, splash.rows().len()),
            },
            Stage::Recovery(recovery) => (recovery.cursor, recovery.rows().len()),
            // Neither holds a list. The offline summary offers one gesture and prints
            // it — `Enter collect` — so a caret would be pointing at the only row.
            Stage::Game(_) | Stage::Offline { .. } => return,
        };
        let Ok(len) = isize::try_from(len) else {
            return;
        };
        if len == 0 {
            return;
        }
        let moved = match isize::try_from(cursor) {
            Ok(cursor) => usize::try_from((cursor + by).rem_euclid(len)).unwrap_or(0),
            Err(_) => 0,
        };
        match &mut self.stage {
            Stage::Splash(splash) => match &mut splash.confirm {
                Some(cursor) => *cursor = moved,
                None => splash.cursor = moved,
            },
            Stage::Recovery(recovery) => recovery.cursor = moved,
            Stage::Game(_) | Stage::Offline { .. } => {}
        }
    }

    /// Which row the caret is on, as a decision rather than as an index.
    fn chosen(&self) -> Option<Chosen> {
        match &self.stage {
            Stage::Splash(splash) => {
                if let Some(cursor) = splash.confirm {
                    return match CONFIRM_ROWS.get(cursor)? {
                        ConfirmRow::Keep => Some(Chosen::Cancel),
                        ConfirmRow::StartOver => Some(Chosen::NewGame),
                    };
                }
                match splash.rows().get(splash.cursor)? {
                    SplashRow::Continue => Some(Chosen::Resume),
                    // **The box appears only where it protects something.** A fresh
                    // install and a title reached through recovery both have nothing to
                    // lose, and a confirmation there would be a question with one
                    // answer.
                    SplashRow::NewGame if splash.resume.is_some() => Some(Chosen::AskNewGame),
                    SplashRow::NewGame => Some(Chosen::NewGame),
                    SplashRow::Quit => Some(Chosen::Quit),
                }
            }
            Stage::Recovery(recovery) => match recovery.rows().get(recovery.cursor)? {
                RecoveryRow::RestoreBackup => Some(Chosen::RestoreBackup),
                RecoveryRow::NewGame => Some(Chosen::NewGame),
                RecoveryRow::Quit => Some(Chosen::Quit),
            },
            Stage::Offline { .. } => Some(Chosen::Collect),
            Stage::Game(_) => None,
        }
    }

    /// Takes the row the caret is on.
    fn confirm(&mut self, now: SystemTime) {
        match self.chosen() {
            Some(Chosen::Resume) => self.resume_run(now),
            Some(Chosen::NewGame) => self.start_new_run(now),
            Some(Chosen::RestoreBackup) => self.restore_backup(now),
            Some(Chosen::Collect) => self.collect_offline(),
            Some(Chosen::AskNewGame) => self.ask_new_game(),
            Some(Chosen::Cancel) => self.cancel(),
            Some(Chosen::Quit) => self.quit = true,
            None => {}
        }
    }

    /// Loads the save the title was built from, and enters it.
    ///
    /// **It re-reads the file rather than keeping the run in memory.** The cost is one
    /// read; what it buys is that `Continue` means the same thing after a launch and
    /// after a walk back to the title, and that a file which broke *between* the two
    /// is routed to recovery instead of being played out of a stale copy.
    fn resume_run(&mut self, now: SystemTime) {
        let Stage::Splash(splash) = &self.stage else {
            return;
        };
        let Some(resume) = &splash.resume else {
            return;
        };
        let (source, from_backup) = (resume.source.clone(), resume.from_backup);
        match persist::load(&source) {
            Ok(Some(save)) => self.open_game(save.state, save.config, from_backup, now),
            // It was there a moment ago and is not now, or no longer loads. That is
            // exactly the question `look` answers, so it answers it again rather than
            // this branch inventing a second routing.
            Ok(None) | Err(_) => self.stage = Self::look(self.slots.as_ref(), now),
        }
    }

    /// Raises the *"start a new game?"* box over the title.
    ///
    /// The caret opens on `Keep`, so the answer a stray `Enter` gives is the one that
    /// changes nothing.
    fn ask_new_game(&mut self) {
        if let Stage::Splash(splash) = &mut self.stage {
            splash.confirm = Some(0);
        }
    }

    /// Takes the box back down, having changed nothing.
    ///
    /// Silent everywhere else: `Esc` on a title with no question up is a key the player
    /// pressed at nothing, and there is no screen behind the title to fall back to.
    fn cancel(&mut self) {
        if let Stage::Splash(splash) = &mut self.stage {
            splash.confirm = None;
        }
    }

    /// Starts a run with no history behind it.
    fn start_new_run(&mut self, now: SystemTime) {
        self.open_game(
            GameState::new(seed_from(now), now),
            Config::default(),
            false,
            now,
        );
    }

    /// Dismisses the offline summary and walks into the run behind it.
    ///
    /// **It collects nothing.** The ore was credited by
    /// [`resume`](skylode_core::game::GameState::resume) before this state was built,
    /// and written to disk in the same breath — so a player who closes the terminal
    /// while reading the summary keeps every block of it. What `Enter` dismisses is a
    /// receipt.
    ///
    /// [`mem::replace`](std::mem::replace) because moving the [`App`] out of one
    /// variant and into another needs to *own* the stage, and `&mut self` only lends
    /// it. The value left behind is never observed: it is overwritten on the next line,
    /// and nothing between the two can look.
    fn collect_offline(&mut self) {
        let stage = std::mem::replace(&mut self.stage, Stage::Splash(Splash::fresh(false)));
        self.stage = match stage {
            Stage::Offline { app, .. } => Stage::Game(app),
            other => other,
        };
    }

    /// §8.3's `Rec -> BakMac` edge: the player asked for the backup.
    fn restore_backup(&mut self, now: SystemTime) {
        let Some(slots) = &self.slots else {
            return;
        };
        self.stage = Self::from_backup(slots, now);
    }

    /// Opens a game over `state`, and says what the player should know on the way in.
    ///
    /// The toasts here are conditions rather than events, and each is told at the
    /// moment its consequence begins. `from_backup` is `docs/UI.md` §8.3's question
    /// left open — *does the game say so when it continues from the backup?* — closed
    /// on a toast and not a frame: a frame exists to ask something, and the player
    /// would answer "yes" every time.
    ///
    /// **It writes immediately**, before the player has pressed anything. A run that
    /// only reached the disk once a cadence came round would mean a `New game`
    /// abandoned in its first seconds leaves a title with nothing to continue —
    /// and, once the offline summary lands, a resume that credited an absence in
    /// memory and could be paid for a second time on the next launch.
    fn open_game(
        &mut self,
        mut state: GameState,
        config: Config,
        from_backup: bool,
        now: SystemTime,
    ) {
        // **Every path, including a brand-new run**, and that costs nothing: a run built
        // a moment ago has `last_seen == now`, so `resume` answers `None` on a span of
        // zero. One rule beats a condition that would have to be kept in step with the
        // ways a game can open.
        let report = state.resume(now);
        let app = App::new(state).with_config(config);
        #[cfg(debug_assertions)]
        let app = app.with_dev(self.dev);
        let mut app = app;

        if from_backup {
            // Neutral and not a refusal: nothing was denied, and the loss is the few
            // seconds between the last save and the one that did not finish.
            app.toasts.push(
                "Restored from the backup save".to_owned(),
                Tone::Neutral,
                TOAST_TTL,
            );
        }
        if self.slots.is_none() {
            // The tone the game uses for *"you cannot have this"*, which is what this
            // is: the session will play and will not be kept.
            app.toasts.push(
                "No save file: this session will not be kept".to_owned(),
                Tone::Refusal,
                TOAST_TTL,
            );
        }
        // **The summary appears when the report *paid* something**, and that is derived
        // rather than a threshold. `resume` already answers `None` on a backward clock
        // and on a span of zero; what is left is the case `q` then `Continue` creates —
        // three seconds of absence, a real report, and nothing in it, because the
        // auto-miner credits whole blocks and three seconds completes none. `gained`
        // being empty *is* that question, so no number is written down here.
        //
        // This corrects `docs/UI.md` §8.3, which draws the edge as `elapsed = 0`.
        let app = Box::new(app);
        self.stage = match report {
            Some(report) if !report.gained.is_empty() => Stage::Offline { app, report },
            _ => Stage::Game(app),
        };

        // The clock is set *before* the write and from the same instant, so the first
        // autosave of a run falls a full period after it opened rather than counting
        // from whenever the title screen happened to be built.
        self.next_autosave = Instant::now() + AUTOSAVE_PERIOD;
        // **And the write covers the offline credit.** `resume` moved the mark and added
        // the ore in memory; a crash before the first cadence would otherwise measure
        // the next absence from the old mark and pay for the same hours twice.
        self.autosave(now);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        backend::TestBackend,
        buffer::Buffer,
        crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    };
    use skylode_core::material::Material;
    use tempfile::TempDir;

    use super::*;

    /// The seed every test session starts from.
    ///
    /// Any value would do; what matters is that it is *fixed*. `GameState::new` draws
    /// the opening mine's whole grid from it, so a seed off the clock would hand each
    /// run of the suite a different picture.
    ///
    /// Spelled here as well as in `app`'s own tests rather than shared: a test fixture
    /// reached across module boundaries would need the test module itself made
    /// `pub(crate)`, which is a larger seam than one constant is worth.
    const SEED: u64 = 0x5B1_0DE;

    /// The instant a fixture is written at, and the one a boot reads.
    ///
    /// **The real clock and not `UNIX_EPOCH`**, unlike `persist`'s own tests. A menu
    /// confirmation reads `SystemTime::now()` at the moment it opens a game — that is
    /// what production does, and the session deliberately holds no injected clock — so
    /// a fixture stamped in 1970 would come back as a fifty-year absence and open an
    /// offline summary in front of every test in this file.
    ///
    /// Nothing here asserts on the *bytes* of a save, which is what makes a moving
    /// instant harmless: the file's determinism is `persist`'s own test to make.
    fn now() -> SystemTime {
        SystemTime::now()
    }

    /// A session with no disk behind it at all.
    fn sessionless() -> Session {
        Session::boot(None, now())
    }

    /// An empty temporary directory and the two slots inside it.
    ///
    /// The [`TempDir`] is returned alongside because dropping it deletes the tree, so
    /// a test that let it go would be reading a directory that no longer exists.
    fn empty() -> (TempDir, SaveSlots) {
        let dir = match TempDir::new() {
            Ok(dir) => dir,
            Err(error) => unreachable!("a temporary directory should be creatable: {error}"),
        };
        let slots = SaveSlots::in_dir(dir.path());
        (dir, slots)
    }

    /// The same, with `writes` runs already written into it.
    ///
    /// Two writes are what a test needs to have a *backup*: the first fills the
    /// primary, the second rotates it aside. That is the real sequence rather than a
    /// file placed by hand, which is the point — the fixture is made the way the game
    /// makes one.
    fn saved(writes: usize) -> (TempDir, SaveSlots) {
        let (dir, slots) = empty();
        for _ in 0..writes {
            let mut state = GameState::new(SEED, now());
            if let Err(error) = persist::save(&slots, &mut state, &Config::default(), now()) {
                unreachable!("the fixture should have been written: {error}");
            }
        }
        (dir, slots)
    }

    /// Every row of the frame, joined — for "is this text on screen anywhere".
    fn whole_frame(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// An event source that reads from a script instead of from a terminal.
    ///
    /// The whole reason [`Session::run`] is generic. It hands out the scripted events
    /// in order and then returns an error, which is how the loop is made to stop even
    /// if the script never quits: a real `EventHandler` blocks forever waiting for a
    /// key that a test will never press, so "the script ran out" has to be a *failure*
    /// rather than a silence. Every test below asserts on the state after `run`
    /// returns, so which of the two ways it ended is checked explicitly.
    ///
    /// `Cell` and not `&mut self`: [`Events::next`] takes `&self` — the real receiver
    /// needs no exclusive borrow — so the cursor has to be interior-mutable. `Cell`
    /// rather than `RefCell` because a `usize` is `Copy` and there is nothing to
    /// borrow, which makes the read a plain load and not a runtime borrow check.
    struct Script {
        events: Vec<Event>,
        next: std::cell::Cell<usize>,
    }

    impl Script {
        fn new(events: Vec<Event>) -> Self {
            Self {
                events,
                next: std::cell::Cell::new(0),
            }
        }
    }

    impl Events for Script {
        fn next(&self) -> Result<Event> {
            let index = self.next.get();
            self.next.set(index + 1);
            self.events
                .get(index)
                .copied()
                .ok_or_else(|| color_eyre::eyre::eyre!("the script ran out"))
        }
    }

    /// A key press with no modifiers, as the event source would report it.
    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// `Ctrl-C` — the only key that ends the process from *inside* a game.
    ///
    /// Every script below that finishes in a run ends on this rather than on `q`,
    /// which now walks back to the title. That is the whole of what this commit
    /// changed, and the scripts are where it shows.
    fn ctrl_c() -> Event {
        Event::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
    }

    /// Runs the real loop over `events`, into an off-screen terminal of that size.
    fn run_sized(
        session: Session,
        width: u16,
        height: u16,
        events: Vec<Event>,
    ) -> (Result<()>, Buffer) {
        let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        let result = session.run(&mut terminal, Script::new(events));
        (result, terminal.backend().buffer().clone())
    }

    /// The same at 80×24, which is every test that is not about the budget.
    ///
    /// Hands back the terminal's buffer too, so a test can read what the *last* frame
    /// drew — the loop draws before every wait, so the buffer after `run` is the frame
    /// the player was looking at when they quit.
    fn run_script(session: Session, events: Vec<Event>) -> (Result<()>, Buffer) {
        run_sized(session, 80, 24, events)
    }

    /// Straight into a game from a session with nothing on disk: `New game`, `Enter`.
    fn into_a_new_game(extra: Vec<Event>) -> (Result<()>, Buffer) {
        let mut events = vec![key(KeyCode::Enter)];
        events.extend(extra);
        run_script(sessionless(), events)
    }

    #[test]
    fn a_launch_with_nothing_on_disk_opens_the_title_without_continue() {
        let (result, buffer) = run_script(sessionless(), vec![key(KeyCode::Char('q'))]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("New game"), "{frame}");
        assert!(
            !frame.contains("Continue"),
            "a fresh install was offered a run to continue: {frame}"
        );
    }

    #[test]
    fn a_session_that_cannot_save_says_so_on_the_title_and_again_in_the_game() {
        let (_, title) = run_script(sessionless(), vec![key(KeyCode::Char('q'))]);
        assert!(
            whole_frame(&title).contains("could not find a place to save"),
            "{}",
            whole_frame(&title)
        );

        let (_, game) = into_a_new_game(vec![ctrl_c()]);
        assert!(
            whole_frame(&game).contains("will not be kept"),
            "{}",
            whole_frame(&game)
        );
    }

    #[test]
    fn a_launch_over_a_save_offers_to_continue_it_and_says_what_it_is() {
        let (_dir, slots) = saved(1);
        let (result, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Char('q'))],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("Continue"), "{frame}");
        // The summary is the save's, not a placeholder: a fresh run is level 1 with a
        // wooden pickaxe, and the title has to say so rather than say `Lv 23`.
        assert!(frame.contains("Lv 1"), "{frame}");
        assert!(frame.contains("Wooden Pickaxe"), "{frame}");
    }

    #[test]
    fn continue_re_reads_the_file_and_lands_in_the_run_it_holds() {
        let (_dir, slots) = saved(1);
        let (result, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Enter), ctrl_c()],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("1 Mine"), "not in a game: {frame}");
        assert!(frame.contains("Haul"), "{frame}");
    }

    #[test]
    fn the_caret_walks_the_menu_and_wraps_at_both_ends() {
        // Two rows on a fresh install: `New game`, `Quit`. Stepping *up* from the top
        // must reach `Quit` — which is what the wrap is for, and what a plain `%` gets
        // wrong on a negative index.
        let (result, _) = run_script(sessionless(), vec![key(KeyCode::Up), key(KeyCode::Enter)]);
        assert!(
            result.is_ok(),
            "the caret did not wrap onto Quit: {result:?}"
        );
    }

    #[test]
    fn a_key_nothing_is_bound_to_on_the_title_changes_nothing() {
        let (result, buffer) = run_script(
            sessionless(),
            vec![key(KeyCode::Char('z')), key(KeyCode::Char('q'))],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
        assert!(whole_frame(&buffer).contains("New game"), "the menu moved");
    }

    #[test]
    fn a_missing_save_with_a_live_backup_is_not_a_fresh_install() {
        // The window the atomic write deliberately keeps: two renames, and a crash
        // between them leaves the backup holding the run and the primary gone.
        let (_dir, slots) = saved(1);
        if let Err(error) = std::fs::rename(slots.primary(), slots.backup()) {
            unreachable!("the fixture should have been movable: {error}");
        }

        let (result, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Char('q'))],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
        assert!(
            whole_frame(&buffer).contains("Continue"),
            "a good run was walked past: {}",
            whole_frame(&buffer)
        );
    }

    #[test]
    fn continuing_from_the_backup_says_so_on_the_way_in() {
        let (_dir, slots) = saved(1);
        if let Err(error) = std::fs::rename(slots.primary(), slots.backup()) {
            unreachable!("the fixture should have been movable: {error}");
        }

        let (_, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Enter), ctrl_c()],
        );
        assert!(
            whole_frame(&buffer).contains("Restored from the backup"),
            "{}",
            whole_frame(&buffer)
        );
    }

    #[test]
    fn an_edited_save_opens_the_recovery_screen_and_offers_the_backup() {
        // Two writes, so there is a backup to offer.
        let (_dir, slots) = saved(2);
        persist::fixture::tamper(slots.primary());

        let (result, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Char('q'))],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("does not match its checksum"), "{frame}");
        assert!(frame.contains("Restore the backup"), "{frame}");
    }

    #[test]
    fn restoring_the_backup_returns_to_the_title_over_the_run_it_holds() {
        let (_dir, slots) = saved(2);
        persist::fixture::tamper(slots.primary());

        // `Enter` on the first row is `Restore the backup`; §8.3 lands that on the
        // title, where `Continue` is offered because the file has just been trusted.
        let (_, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Enter), key(KeyCode::Char('q'))],
        );
        let frame = whole_frame(&buffer);
        assert!(frame.contains("Continue"), "{frame}");
        assert!(frame.contains("Lv 1"), "{frame}");
    }

    #[test]
    fn both_files_broken_leaves_nothing_to_restore() {
        let (_dir, slots) = saved(2);
        persist::fixture::tamper(slots.primary());
        persist::fixture::tamper(slots.backup());

        let (_, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Down), key(KeyCode::Char('q'))],
        );
        let frame = whole_frame(&buffer);
        assert!(frame.contains("Restore the backup"), "{frame}");

        // Asking for it anyway is the diagram's `BakMac -> RecNoBak` edge.
        let (_dir2, slots2) = saved(2);
        persist::fixture::tamper(slots2.primary());
        persist::fixture::tamper(slots2.backup());
        let (_, after) = run_script(
            Session::boot(Some(slots2), now()),
            vec![key(KeyCode::Enter), key(KeyCode::Char('q'))],
        );
        let frame = whole_frame(&after);
        assert!(frame.contains("neither does the backup"), "{frame}");
        assert!(!frame.contains("Restore the backup"), "{frame}");
    }

    #[test]
    fn a_save_from_the_future_offers_nothing_but_quitting() {
        let (_dir, slots) = saved(1);
        persist::fixture::from_the_future(slots.primary());

        let (_, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Char('q'))],
        );
        let frame = whole_frame(&buffer);
        assert!(frame.contains("newer version of Skylode"), "{frame}");
        // Not offered: an older build starting a run over it would overwrite a save
        // that was never broken.
        assert!(!frame.contains("Start a new game"), "{frame}");
        assert!(!frame.contains("Restore the backup"), "{frame}");
    }

    #[test]
    fn the_loop_draws_before_it_waits() {
        // The first frame must be on screen *before* the first event is asked for,
        // or the player stares at a blank terminal until they touch a key. Asserted
        // by giving the loop a script that quits on its very first event: if drawing
        // came second, nothing would ever have been painted.
        let (result, buffer) = into_a_new_game(vec![ctrl_c()]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("1 Mine"), "nothing was drawn: {frame}");
    }

    #[test]
    fn a_key_is_decoded_and_applied_before_the_next_frame() {
        // The loop's real job: key → `keymap::resolve` → `update` → redraw. Two tab
        // presses then a quit, and the frame left on screen has to be the third
        // screen of the ring — proof the keys went through the reducer and that the
        // redraw happened after them rather than before.
        let (result, buffer) =
            into_a_new_game(vec![key(KeyCode::Tab), key(KeyCode::Tab), ctrl_c()]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("Inventory"), "{frame}");
        assert!(
            frame.contains("Compressible now"),
            "not on Inventory: {frame}"
        );
    }

    #[test]
    fn a_key_nothing_is_bound_to_leaves_the_session_alone() {
        // `resolve` returns `None` and the loop must simply go round again — not
        // quit, not panic, not swallow the next event.
        let (result, buffer) = into_a_new_game(vec![key(KeyCode::Char('z')), ctrl_c()]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        assert!(whole_frame(&buffer).contains("Haul"), "the screen moved");
    }

    #[test]
    fn a_tick_and_a_resize_both_go_round_the_loop_without_changing_the_screen() {
        // `Tick` runs the heartbeat and `Resize` does nothing at all — ratatui lays
        // out against the new size on the next draw, which the loop is about to do
        // anyway. Both must still reach the quit behind them, which is what fails if
        // either arm ever starts returning early.
        let (result, buffer) =
            into_a_new_game(vec![Event::Tick, Event::Resize, Event::Tick, ctrl_c()]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        assert!(whole_frame(&buffer).contains("Haul"), "the screen moved");
    }

    #[test]
    fn a_dead_event_source_stops_the_loop_instead_of_spinning() {
        // The other way out. A real `EventHandler` whose thread has died closes the
        // channel, and `recv` then fails forever — so the `?` on `events.next()` has
        // to end the loop rather than let it spin on an error it ignores. The script
        // reproduces that by running out.
        let (result, _) = run_script(sessionless(), vec![Event::Tick]);
        assert!(result.is_err(), "the loop kept going past a dead source");
    }

    #[test]
    fn a_cramped_terminal_shows_the_filter_over_every_state() {
        // The filter is above the machine, so it must win over the title as surely as
        // over a game — and `q` under it has to quit the process, which is the only
        // affordance the frame prints.
        let (result, buffer) = run_sized(sessionless(), 54, 18, vec![key(KeyCode::Char('q'))]);
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("Skylode needs 80 x 24"), "{frame}");
        assert!(
            !frame.contains("New game"),
            "the title leaked through: {frame}"
        );
    }

    #[test]
    fn a_key_the_cramped_screen_does_not_print_does_nothing() {
        // The frame offers `q` and nothing else, so `Enter` must not confirm a menu
        // row nobody can see.
        let (result, _) = run_sized(
            sessionless(),
            54,
            18,
            vec![key(KeyCode::Enter), key(KeyCode::Char('q'))],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
    }

    /// Slots pointing *through* a file, so every read and every write fails with an
    /// `Io` — the portable way to break a disk without touching permissions.
    fn unreachable_slots(dir: &TempDir) -> SaveSlots {
        let blocker = dir.path().join("a-file-not-a-directory");
        if let Err(error) = std::fs::write(&blocker, "in the way") {
            unreachable!("the fixture should have been writable: {error}");
        }
        SaveSlots::in_dir(&blocker.join("skylode"))
    }

    /// The toasts of the game a session is showing, or nothing when it is not showing
    /// one.
    fn announcements(session: &Session) -> Vec<String> {
        match &session.stage {
            Stage::Game(app) | Stage::Offline { app, .. } => app
                .toasts
                .log(Instant::now())
                .map(|(_, text)| text.to_string())
                .collect(),
            Stage::Splash(_) | Stage::Recovery(_) => Vec::new(),
        }
    }

    #[test]
    fn a_new_run_is_on_the_disk_before_the_player_presses_anything() {
        // Not the cadence but its floor: a `New game` abandoned in its first seconds
        // must still leave a title with something to continue.
        let (_dir, slots) = empty();
        let readable = slots.clone();
        let (result, _) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Enter), ctrl_c()],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
        assert!(
            matches!(persist::load(readable.primary()), Ok(Some(_))),
            "the run never reached the disk"
        );
    }

    #[test]
    fn a_write_that_fails_on_the_way_in_is_announced_and_does_not_end_the_session() {
        // A broken disk found at boot lands on the recovery screen, whose first row is
        // `Start a new game` — so this is the real path to a run that cannot be saved.
        let (dir, _) = empty();
        let slots = unreachable_slots(&dir);
        let (result, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Enter), ctrl_c()],
        );
        assert!(
            result.is_ok(),
            "a failed write ended the session: {result:?}"
        );
        let frame = whole_frame(&buffer);
        assert!(frame.contains("Save failed"), "{frame}");
        // And the game is still there behind the toast: a run in memory is fine, and
        // throwing it away is what "no continue anyway" exists to refuse.
        assert!(frame.contains("Haul"), "{frame}");
    }

    #[test]
    fn a_failing_write_is_announced_once_and_so_is_its_recovery() {
        let (good_dir, good) = empty();
        let (bad_dir, _) = empty();
        let bad = unreachable_slots(&bad_dir);

        let mut session = Session::boot(Some(good.clone()), now());
        // `Enter` on a fresh title is `New game`, which opens a run and writes it.
        session.menu(MenuAction::Confirm);
        assert!(
            announcements(&session).is_empty(),
            "a healthy write spoke up"
        );

        session.slots = Some(bad);
        session.autosave(now());
        assert_eq!(announcements(&session).len(), 1, "the failure went unsaid");
        assert!(announcements(&session)[0].contains("Save failed"));

        // Still broken: the player has already been told, and a toast every ten
        // seconds would bury the game under identical refusals.
        session.autosave(now());
        assert_eq!(
            announcements(&session).len(),
            1,
            "the failure repeated itself"
        );

        session.slots = Some(good);
        session.autosave(now());
        let said = announcements(&session);
        assert_eq!(said.len(), 2, "the recovery went unsaid");
        assert!(said[0].contains("Saving works again"), "{said:?}");

        // And once more, silent again — the edge is what is announced, not the state.
        session.autosave(now());
        assert_eq!(announcements(&session).len(), 2);
        drop(good_dir);
    }

    #[test]
    fn a_session_with_nowhere_to_write_never_pretends_to_have_written() {
        let mut session = sessionless();
        session.menu(MenuAction::Confirm);
        // One toast — the standing "this will not be kept" — and never a save failure,
        // because nothing was attempted: without a `SaveSlots` there is no way to ask.
        let said = announcements(&session);
        assert_eq!(said.len(), 1, "{said:?}");
        assert!(said[0].contains("will not be kept"), "{said:?}");
        session.autosave(now());
        assert_eq!(
            announcements(&session).len(),
            1,
            "a phantom write reported in"
        );
    }

    #[test]
    fn nothing_is_written_from_a_screen_that_has_no_run() {
        let (_dir, slots) = empty();
        let mut session = Session::boot(Some(slots.clone()), now());
        session.autosave(now());
        assert!(
            matches!(persist::load(slots.primary()), Ok(None)),
            "a title screen wrote a save"
        );
    }

    #[test]
    fn a_dead_event_source_still_banks_the_run() {
        // The `?` on `events.next()` used to lose whatever the last ten seconds held.
        // The script runs out, which is what a dead `EventHandler` looks like.
        let (_dir, slots) = empty();
        let readable = slots.clone();
        let (result, _) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Enter), Event::Tick],
        );
        assert!(result.is_err(), "the loop kept going past a dead source");
        assert!(
            matches!(persist::load(readable.primary()), Ok(Some(_))),
            "the run was dropped with the channel"
        );
    }

    #[test]
    fn q_in_a_game_goes_back_to_the_title_rather_than_out_of_the_program() {
        // §8.3's `Game -> Splash` edge, walked: `Enter` starts a run, `q` puts it down,
        // and the title behind it offers to continue — which it can only do because the
        // run was written on the way out and read back on the way in.
        let (_dir, slots) = empty();
        let (result, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![
                key(KeyCode::Enter),
                key(KeyCode::Char('q')),
                key(KeyCode::Char('q')),
            ],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(
            frame.contains("Continue"),
            "the title did not find the run it had just left: {frame}"
        );
        assert!(frame.contains("Lv 1"), "{frame}");
    }

    #[test]
    fn leaving_for_the_title_writes_the_run_before_re_reading_it() {
        // The ordering `on_key` depends on. A save that happened *after* the re-read
        // would build a title out of the previous file, so the level the title shows is
        // the assertion: the dev menu takes the run to level 12, `q` writes it, and the
        // title has to have read that.
        #[cfg(debug_assertions)]
        {
            let (_dir, slots) = empty();
            let mut session = Session::boot(Some(slots.clone()), now()).with_dev(true);
            session.menu(MenuAction::Confirm);
            if let Stage::Game(app) = &mut session.stage {
                app.state.dev_set_level(12);
            }
            session.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));

            match &session.stage {
                Stage::Splash(splash) => {
                    let level = splash.resume().map(Resume::level);
                    assert_eq!(level, Some(12), "the title read a stale file");
                }
                _ => unreachable!("`q` did not land on the title"),
            }
        }
    }

    /// A save written as if the run had been put down `ago` before `now()`.
    ///
    /// The absence is made by writing at an *earlier* instant rather than by moving a
    /// clock, which is what keeps the test independent of when it runs: `persist::save`
    /// stamps the run with the `now` it is handed, so the boot's own `now()` is that much
    /// later by construction.
    fn saved_and_left(ago: Duration) -> (TempDir, SaveSlots) {
        let (dir, slots) = empty();
        let mut state = GameState::new(SEED, now());
        let written = now().checked_sub(ago).unwrap_or(now());
        if let Err(error) = persist::save(&slots, &mut state, &Config::default(), written) {
            unreachable!("the fixture should have been written: {error}");
        }
        (dir, slots)
    }

    #[test]
    fn an_absence_that_paid_something_opens_the_summary_before_the_game() {
        let (_dir, slots) = saved_and_left(Duration::from_secs(6 * 3_600));
        let (result, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Enter), ctrl_c()],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
        let frame = whole_frame(&buffer);
        assert!(frame.contains("Welcome back"), "{frame}");
        assert!(frame.contains("You were away for  6h"), "{frame}");
        // The run is drawn under it, which is what makes this a modal.
        assert!(frame.contains("1 Mine"), "{frame}");
    }

    #[test]
    fn enter_dismisses_the_summary_and_leaves_the_run_running() {
        let (_dir, slots) = saved_and_left(Duration::from_secs(6 * 3_600));
        let (_, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Enter), key(KeyCode::Enter), ctrl_c()],
        );
        let frame = whole_frame(&buffer);
        assert!(
            !frame.contains("Welcome back"),
            "the receipt stayed up: {frame}"
        );
        assert!(frame.contains("Haul"), "{frame}");
    }

    #[test]
    fn an_absence_too_short_to_pay_for_a_block_shows_no_screen_at_all() {
        // The `q` then `Continue` case, which is why the rule is "the report paid
        // something" and not "elapsed > 0": three seconds is a real report with nothing
        // in it, because the auto-miner credits whole blocks.
        let (_dir, slots) = saved_and_left(Duration::from_secs(3));
        let (_, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Enter), ctrl_c()],
        );
        assert!(
            !whole_frame(&buffer).contains("Welcome back"),
            "three seconds away opened a summary: {}",
            whole_frame(&buffer)
        );
    }

    #[test]
    fn the_offline_credit_is_on_the_disk_before_the_player_reads_it() {
        // `resume` moves the mark and adds the ore *in memory*. A crash before the
        // first cadence would otherwise measure the next absence from the old mark and
        // pay for the same six hours twice.
        let (_dir, slots) = saved_and_left(Duration::from_secs(6 * 3_600));
        let mut session = Session::boot(Some(slots.clone()), now());
        session.menu(MenuAction::Confirm);
        assert!(
            matches!(session.stage, Stage::Offline { .. }),
            "the summary never opened"
        );

        let reloaded = match persist::load(slots.primary()) {
            Ok(Some(save)) => save,
            other => unreachable!("the run should have been on disk: {other:?}"),
        };
        // Two halves of one write, and both have to be in it. The ore, or the six hours
        // bought nothing; and the *mark*, or the next launch would measure the absence
        // from where it already started and pay for them again.
        //
        // The mark is checked as a span rather than for equality because the test does
        // real disk I/O: a second or two passes between the write and this read, and a
        // second is enough for the auto-miner's carry to complete another block. What
        // must not have survived is the six hours.
        let state = reloaded.state;
        assert!(
            state.player().get_inventory().raw_value(Material::Stone) > 0,
            "the absence credited nothing"
        );
        let since_written = now().duration_since(state.last_seen()).unwrap_or_default();
        assert!(
            since_written < Duration::from_secs(60),
            "the file still says the run was left {since_written:?} ago"
        );
    }

    #[test]
    fn the_summary_pauses_the_tick_and_holds_no_live_flash() {
        // Phase 7's note, answered by construction rather than by a `clear`: this is the
        // first state that pauses a tick, and the only `App` it can hold has just been
        // built from a load — so there is no beat mid-flight to freeze.
        let (_dir, slots) = saved_and_left(Duration::from_secs(6 * 3_600));
        let mut session = Session::boot(Some(slots), now());
        session.menu(MenuAction::Confirm);

        assert!(
            !session.advance(Instant::now()),
            "the tick ran behind the box"
        );
        match &session.stage {
            Stage::Offline { app, .. } => {
                let mine = app.state.current_mine().kind();
                assert!(
                    app.flash.resolve(mine, Instant::now()).is_empty(),
                    "a flash was left mid-beat under a paused tick"
                );
            }
            _ => unreachable!("the summary never opened"),
        }
    }

    #[test]
    fn new_game_over_a_save_asks_before_it_writes_over_anything() {
        let (_dir, slots) = saved(1);
        // Down onto `New game`, then `Enter`: the box goes up and the run is untouched.
        let (result, buffer) = run_script(
            Session::boot(Some(slots.clone()), now()),
            vec![
                key(KeyCode::Down),
                key(KeyCode::Enter),
                key(KeyCode::Char('q')),
            ],
        );
        assert!(result.is_ok(), "the loop failed: {result:?}");
        assert!(
            whole_frame(&buffer).contains("Start a new game?"),
            "{}",
            whole_frame(&buffer)
        );
    }

    #[test]
    fn escaping_the_box_leaves_the_menu_exactly_as_it_was() {
        let (_dir, slots) = saved(1);
        let mut session = Session::boot(Some(slots), now());
        session.menu(MenuAction::Down);
        session.menu(MenuAction::Confirm);
        session.menu(MenuAction::Cancel);

        match &session.stage {
            Stage::Splash(splash) => {
                assert!(splash.confirm().is_none(), "the box stayed up");
                // And the caret is still on `New game`, not reset to the top: backing
                // out is not the same gesture as starting again.
                assert_eq!(
                    splash.rows().get(splash.cursor()),
                    Some(&SplashRow::NewGame)
                );
            }
            _ => unreachable!("`Esc` left the title"),
        }
    }

    #[test]
    fn answering_yes_starts_the_run_the_box_warned_about() {
        let (_dir, slots) = saved(1);
        let mut session = Session::boot(Some(slots.clone()), now());
        session.menu(MenuAction::Down);
        session.menu(MenuAction::Confirm);
        // `Down` onto `Yes, start over`, then take it.
        session.menu(MenuAction::Down);
        session.menu(MenuAction::Confirm);

        assert!(
            matches!(session.stage, Stage::Game(_)),
            "no run was started"
        );
        // A brand-new run is level 1 and, per the immediate write, already on disk in
        // place of the old one.
        match persist::load(slots.primary()) {
            Ok(Some(save)) => assert_eq!(save.state.player().get_level(), 1),
            other => unreachable!("the new run should have been written: {other:?}"),
        }
    }

    #[test]
    fn taking_the_boxs_first_row_keeps_the_save_exactly_as_esc_does() {
        // Two ways to decline and they must agree: `Esc` above, and `Enter` on the row
        // the caret opens on.
        let (_dir, slots) = saved(1);
        let mut session = Session::boot(Some(slots), now());
        session.menu(MenuAction::Down);
        session.menu(MenuAction::Confirm);
        session.menu(MenuAction::Confirm);

        match &session.stage {
            Stage::Splash(splash) => assert!(splash.confirm().is_none(), "the box stayed up"),
            _ => unreachable!("declining left the title"),
        }
    }

    #[test]
    fn quitting_from_a_recovery_frame_ends_the_session() {
        // The third row, walked to. It is the only way out of that screen that does not
        // touch either file.
        let (_dir, slots) = saved(2);
        persist::fixture::tamper(slots.primary());
        let (result, _) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Down), key(KeyCode::Down), key(KeyCode::Enter)],
        );
        assert!(result.is_ok(), "the frame did not let go: {result:?}");
    }

    #[test]
    fn a_backup_that_cannot_be_read_leaves_nothing_to_restore() {
        // A *directory* where the backup should be: the primary is simply missing, so
        // the machine looks at the backup — and cannot read it either.
        let (_dir, slots) = empty();
        if let Err(error) = std::fs::create_dir_all(slots.backup()) {
            unreachable!("the fixture should have been creatable: {error}");
        }
        let (_, buffer) = run_script(
            Session::boot(Some(slots), now()),
            vec![key(KeyCode::Down), key(KeyCode::Enter)],
        );
        assert!(
            whole_frame(&buffer).contains("could not be read"),
            "{}",
            whole_frame(&buffer)
        );
    }

    #[test]
    fn the_summary_has_no_caret_to_walk() {
        // `Up` and `Down` reach the offline summary like any other menu key, and there
        // is nothing there for them to move — it offers one gesture and prints it.
        let (_dir, slots) = saved_and_left(Duration::from_secs(6 * 3_600));
        let mut session = Session::boot(Some(slots), now());
        session.menu(MenuAction::Confirm);
        session.menu(MenuAction::Up);
        session.menu(MenuAction::Down);
        assert!(
            matches!(session.stage, Stage::Offline { .. }),
            "a caret gesture moved the summary somewhere"
        );
    }

    #[test]
    fn a_file_that_breaks_between_the_title_and_continue_lands_in_recovery() {
        // The whole reason `Continue` re-reads rather than keeping the run in memory:
        // the title is a picture of the file as it was, and the file can move.
        let (_dir, slots) = saved(2);
        let mut session = Session::boot(Some(slots.clone()), now());
        persist::fixture::tamper(slots.primary());
        session.menu(MenuAction::Confirm);

        assert!(
            matches!(session.stage, Stage::Recovery(_)),
            "a broken file was played out of a stale summary"
        );
    }

    #[test]
    fn a_fresh_install_is_not_asked_a_question_with_one_answer() {
        // The box protects a run. Where there is none — a fresh install, or a title
        // reached through recovery — `New game` acts on the spot.
        let mut session = sessionless();
        session.menu(MenuAction::Confirm);
        assert!(
            matches!(session.stage, Stage::Game(_)),
            "a fresh install was asked to confirm"
        );
    }
}
