//! The save-recovery screens (UI.md §6.3).
//!
//! Recovery **refuses a save that fails its checksum — there is no "continue
//! anyway"**: loading data that failed its HMAC is exactly what a hand-editor needs,
//! and refusing it is a real if partial protection. The innocent player loses
//! seconds, not a run, because the `.bak` is the last save that passed and autosave
//! runs often.
//!
//! Per the bootstrap rule (§2.3) these render with **hardcoded text**, never a
//! `View` or a config: a save that will not load is the one thing that cannot supply
//! its own chrome. What varies is only what the loader found — [`Trouble`] — plus a
//! caret and, where there is a backup to offer, how old it is.
//!
//! ## Four troubles, three frames
//!
//! Two causes share a frame exactly when they share an *answer*.
//! [`NothingLeft`](Trouble::NothingLeft) and [`Unreadable`](Trouble::Unreadable)
//! differ in their first sentence and in nothing else — there is nowhere left to go
//! in either — so they are one shape with two headers rather than two frames that
//! would drift apart.

use ratatui::{Frame, layout::Rect};

use crate::{
    format::age,
    session::{Recovery, RecoveryRow, Trouble},
};

/// How wide every recovery frame is drawn.
const WIDTH: u16 = 66;

/// The column each row's consequence lines up in, so the rows read as a table.
const HINT_COLUMN: usize = 26;

/// Draws the recovery screen for whatever the loader refused.
pub fn render(frame: &mut Frame, area: Rect, recovery: &Recovery) {
    let mut lines: Vec<String> = vec![String::new()];
    lines.extend(explanation(recovery.trouble()));
    lines.push(String::new());
    lines.extend(rows(recovery));
    lines.push(String::new());
    lines.extend(footnote(recovery.trouble()));

    // Two for the borders. Derived rather than written down, so a sentence added to
    // one of the four cases cannot silently fall out of the bottom of the box.
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    super::modal(frame, area, WIDTH, height, " Save problem ", &borrowed);
}

/// What went wrong, in the player's words.
fn explanation(trouble: Trouble) -> Vec<String> {
    match trouble {
        Trouble::BackupOffered { .. } => vec![
            " Your save does not match its checksum.".to_owned(),
            String::new(),
            " Either the file was edited, or a write was interrupted.".to_owned(),
            " Skylode will not load it: the values inside cannot be".to_owned(),
            " trusted, and it will not guess which ones.".to_owned(),
        ],
        Trouble::NothingLeft => vec![
            " Your save does not match its checksum,".to_owned(),
            " and neither does the backup.".to_owned(),
            String::new(),
            " Skylode will not load either of them, and has changed".to_owned(),
            " neither.".to_owned(),
        ],
        Trouble::Unreadable => vec![
            " Your save could not be read at all.".to_owned(),
            String::new(),
            " The file is there and the system refused it: a".to_owned(),
            " permission, a disk that has gone away, a directory".to_owned(),
            " standing where a file should be.".to_owned(),
        ],
        Trouble::FromTheFuture { found, current } => vec![
            " This save was written by a newer version of Skylode.".to_owned(),
            String::new(),
            format!(" It is version {found}; this build reads up to {current}."),
            " Skylode will not open it: a newer save can describe".to_owned(),
            " things these rules do not have.".to_owned(),
        ],
    }
}

/// The rows the player can take, with the caret on the one they are pointing at.
fn rows(recovery: &Recovery) -> Vec<String> {
    recovery
        .rows()
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let caret = if index == recovery.cursor() {
                " ▸  "
            } else {
                "    "
            };
            let label = label(*row);
            match hint(*row, recovery.trouble()) {
                Some(hint) => {
                    let pad = HINT_COLUMN.saturating_sub(label.chars().count());
                    format!("{caret}{label}{}{hint}", " ".repeat(pad))
                }
                None => format!("{caret}{label}"),
            }
        })
        .collect()
}

/// What each row is called.
fn label(row: RecoveryRow) -> &'static str {
    match row {
        RecoveryRow::RestoreBackup => "Restore the backup",
        RecoveryRow::NewGame => "Start a new game",
        RecoveryRow::Quit => "Quit",
    }
}

/// The short consequence printed beside a row.
///
/// **`Start a new game` warns rather than reassures**, and the wording it replaces is
/// why. The frame used to promise *"the current save is kept"*, which is true only
/// until the new run's first autosave: that write rotates the broken file into the
/// backup slot and takes the good backup with it. A sentence that stops being true
/// after ten seconds of play is worse than no sentence.
fn hint(row: RecoveryRow, trouble: Trouble) -> Option<String> {
    match (row, trouble) {
        (RecoveryRow::RestoreBackup, Trouble::BackupOffered { age: Some(since) }) => {
            Some(format!("saved {} ago", age(since.as_secs())))
        }
        (RecoveryRow::NewGame, Trouble::BackupOffered { .. }) => {
            Some("the backup goes with it".to_owned())
        }
        (RecoveryRow::NewGame, _) => Some("both files are written over".to_owned()),
        _ => None,
    }
}

/// The paragraph under the rows: what the player should take from all this.
fn footnote(trouble: Trouble) -> Vec<String> {
    match trouble {
        Trouble::BackupOffered { .. } => vec![
            " The backup is the last save that passed its check, so at".to_owned(),
            " most a few seconds of mining are missing.".to_owned(),
        ],
        Trouble::NothingLeft => vec![
            " If you edited a save by hand, this is why. If you did not,".to_owned(),
            " the disk did — and the files are still there to look at.".to_owned(),
        ],
        Trouble::Unreadable => vec![
            " The backup is no help here: both files live in the same".to_owned(),
            " directory, so whatever stopped one stops the other.".to_owned(),
        ],
        Trouble::FromTheFuture { .. } => vec![
            " Update the game and it will open. Starting again is not".to_owned(),
            " offered: this save is not broken, and an older build".to_owned(),
            " would write over it.".to_owned(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn drawn(trouble: Trouble) -> String {
        let recovery = Recovery::sample(trouble);
        crate::overlay::render_to_string(|frame, area| render(frame, area, &recovery))
    }

    #[test]
    fn the_backup_screen_offers_to_restore_and_refuses_to_load() {
        let frame = drawn(Trouble::BackupOffered {
            age: Some(Duration::from_secs(8)),
        });
        assert!(frame.contains("does not match its checksum"), "{frame}");
        assert!(frame.contains("Restore the backup"), "{frame}");
        assert!(frame.contains("saved 8s ago"), "{frame}");
        assert!(frame.contains("Start a new game"), "{frame}");
    }

    #[test]
    fn a_backup_of_unknown_age_still_gets_offered() {
        // §8.3 makes *trying* the backup the player's move, so the row appears even
        // when there is no file behind it — choosing it is what discovers that.
        let frame = drawn(Trouble::BackupOffered { age: None });
        assert!(frame.contains("Restore the backup"), "{frame}");
        assert!(!frame.contains("saved "), "an age was invented: {frame}");
    }

    #[test]
    fn the_both_failed_screen_changes_nothing_and_offers_no_restore() {
        let frame = drawn(Trouble::NothingLeft);
        assert!(frame.contains("and neither does the backup."), "{frame}");
        assert!(frame.contains("has changed"), "{frame}");
        assert!(!frame.contains("Restore the backup"), "{frame}");
    }

    #[test]
    fn an_unreadable_save_is_not_described_as_a_checksum_failure() {
        // The one wording that had to change: `Io` shares this shape, and the old
        // header would have told a player with a permission problem that their file
        // had been edited.
        let frame = drawn(Trouble::Unreadable);
        assert!(frame.contains("could not be read at all"), "{frame}");
        assert!(!frame.contains("checksum"), "{frame}");
        assert!(frame.contains("Start a new game"), "{frame}");
    }

    #[test]
    fn a_save_from_the_future_names_both_versions_and_offers_only_quitting() {
        let frame = drawn(Trouble::FromTheFuture {
            found: 4,
            current: 1,
        });
        assert!(frame.contains("version 4"), "{frame}");
        assert!(frame.contains("reads up to 1"), "{frame}");
        assert!(frame.contains("Quit"), "{frame}");
        assert!(!frame.contains("Start a new game"), "{frame}");
        assert!(!frame.contains("Restore the backup"), "{frame}");
    }

    #[test]
    fn starting_over_warns_that_the_backup_goes_too() {
        let frame = drawn(Trouble::BackupOffered { age: None });
        assert!(frame.contains("the backup goes with it"), "{frame}");
    }
}
