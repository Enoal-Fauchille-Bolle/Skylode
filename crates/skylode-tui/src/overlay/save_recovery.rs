//! The save-recovery screens (UI.md §6.3).
//!
//! Recovery **refuses a save that fails its checksum — there is no "continue
//! anyway"**: loading data that failed its HMAC is exactly what a hand-editor needs,
//! and refusing it is a real if partial protection. The innocent player loses
//! seconds, not a run, because the `.bak` is the last save that passed and autosave
//! runs often. When the backup fails too there is no floor left, and the second
//! frame says so — both files kept, untouched.
//!
//! Per the bootstrap rule (§2.3) these render with **hardcoded defaults**, never a
//! `View` or config: a save that will not load is the one thing that cannot supply
//! its own chrome. `#[allow(dead_code)]` until the loader raises them (phase 7).

use ratatui::{Frame, layout::Rect};

/// The recovery screen when the backup is still good.
#[allow(dead_code, reason = "awaiting the phase-7 save loader")]
pub fn render_backup(frame: &mut Frame, area: Rect) {
    super::modal(
        frame,
        area,
        66,
        18,
        " Save problem ",
        &[
            "",
            " Your save does not match its checksum.",
            "",
            " Either the file was edited, or a write was interrupted.",
            " Skylode will not load it: the values inside cannot be",
            " trusted, and it will not guess which ones.",
            "",
            " ▸  Restore the backup      saved 8 seconds ago",
            "    Start a new game        the current save is kept",
            "    Quit",
            "",
            " The backup is the last save that passed its check, so at",
            " most a few seconds of mining are missing.",
        ],
    );
}

/// The recovery screen when the backup has failed too — no floor left.
#[allow(dead_code, reason = "awaiting the phase-7 save loader")]
pub fn render_both_failed(frame: &mut Frame, area: Rect) {
    super::modal(
        frame,
        area,
        66,
        16,
        " Save problem ",
        &[
            "",
            " Your save does not match its checksum,",
            " and neither does the backup.",
            "",
            " Both files are kept exactly as they are. Skylode will not",
            " load either of them.",
            "",
            " ▸  Start a new game",
            "    Quit",
            "",
            " If you edited a save by hand, this is why. If you did not,",
            " the disk did — and the files are still there to look at.",
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backup_screen_offers_to_restore_and_refuses_to_load() {
        let frame = crate::overlay::render_to_string(render_backup);
        assert!(frame.contains("does not match its checksum"), "{frame}");
        assert!(frame.contains("Restore the backup"), "{frame}");
        assert!(frame.contains("Start a new game"), "{frame}");
    }

    #[test]
    fn the_both_failed_screen_keeps_the_files_and_offers_no_restore() {
        let frame = crate::overlay::render_to_string(render_both_failed);
        assert!(frame.contains("and neither does the backup."), "{frame}");
        assert!(frame.contains("Both files are kept"), "{frame}");
        assert!(!frame.contains("Restore the backup"), "{frame}");
    }
}
