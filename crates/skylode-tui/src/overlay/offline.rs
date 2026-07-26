//! The offline-summary modal (UI.md §6.4).
//!
//! Shown once on resume, and it **states the multiplication** — `rate × elapsed` is
//! the whole offline mechanic, so printing it makes the number checkable in the
//! player's head. The parenthesised denomination split is the same raw total shown
//! two ways, not a compression. A backward clock shows no screen at all, which is
//! the session machine's business, not this render's.
//!
//! `#[allow(dead_code)]`, not `#[expect]`: the render is live in the test build and
//! dead in the bin until the session state machine raises it (phase 7), and an
//! `expect` would be unfulfilled in whichever of the two builds actually uses it.

use ratatui::{Frame, layout::Rect};

/// Draws the offline summary with placeholder gains (the real report is
/// `GameState::resume`'s, phase 7).
#[allow(dead_code, reason = "awaiting the phase-7 session state machine")]
pub fn render(frame: &mut Frame, area: Rect) {
    super::modal(
        frame,
        area,
        60,
        14,
        " Welcome back ",
        &[
            "",
            " You were away for  6h 12m",
            " The auto-miner kept going.",
            "",
            " +12 480  Iron            (124 Compressed + 80)",
            " +   372  Coal",
            " +    31  Gold",
            "",
            " Rate  0.56 blocks/s  ×  6h 12m",
            "",
            " Enter  collect",
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_shows_the_span_the_gains_and_the_multiplication() {
        let frame = crate::overlay::render_to_string(render);
        assert!(frame.contains("Welcome back"), "{frame}");
        assert!(frame.contains("You were away for  6h 12m"), "{frame}");
        assert!(frame.contains("+12 480  Iron"), "{frame}");
        // The mechanic, spelled out: rate × elapsed.
        assert!(frame.contains("Rate  0.56 blocks/s  ×  6h 12m"), "{frame}");
    }
}
