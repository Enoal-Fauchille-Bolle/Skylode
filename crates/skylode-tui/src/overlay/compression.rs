//! The compression dialog (UI.md §6.6).
//!
//! A bounded spinner (`◄ 12 ►`), not a typed number, because the amount has a known
//! maximum and a spinner cannot be wrong. The inverse — decompress — is the same
//! frame with the arithmetic reversed, which is free-and-lossless-both-ways showing
//! up as a UI economy rather than a second screen.
//!
//! `#[allow(dead_code)]` for the same reason as the offline summary: live in tests,
//! dead in the bin until the Inventory screen opens it (phase 5).

use ratatui::{Frame, layout::Rect};

/// Draws the compression dialog with placeholder amounts (the real spinner is
/// driven by `Inventory::compress`, phase 5).
#[allow(dead_code, reason = "awaiting the phase-5 compression binding")]
pub fn render(frame: &mut Frame, area: Rect) {
    super::modal(
        frame,
        area,
        48,
        11,
        " Compress Iron ",
        &[
            "",
            " Raw held        1 350",
            "",
            " Compress   ◄  12  ►",
            " Costs        1 200 raw",
            " Leaves         150 raw",
            "",
            " a  all (13)   Enter  do it",
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_shows_the_spinner_and_both_amounts() {
        let frame = crate::overlay::render_to_string(render);
        assert!(frame.contains("Compress Iron"), "{frame}");
        assert!(frame.contains("Raw held        1 350"), "{frame}");
        assert!(frame.contains("◄  12  ►"), "{frame}");
        assert!(frame.contains("Costs        1 200 raw"), "{frame}");
        assert!(frame.contains("a  all (13)"), "{frame}");
    }
}
