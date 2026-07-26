//! The two prestige modals: the preview, then the typed confirm (UI.md §6.8–6.9).
//!
//! **The preview is two columns because the reset is a trade.** The left column is
//! deliberately brutal — the deep reset is the point, and a preview that soft-pedals
//! it sets up the one complaint that cannot be undone. It is drawn unaffordable,
//! which is the common case, so the honest last line names the progression still
//! owed rather than a price for an ore that only drops past those gates.
//!
//! **The confirm is the one place in the game that asks for typing.** Everything
//! else is a spinner or a menu because a keystroke cannot be wrong; here a keystroke
//! *being* possible is the point — it must break the training that nothing is final.
//!
//! `#[allow(dead_code)]`: live in tests, dead in the bin until the Stats screen's
//! `p` opens them (phase 7).

use ratatui::{Frame, layout::Rect};

/// Draws the prestige preview with placeholder figures (the real ones are
/// `Player::prestige_lock`'s, phase 7).
#[allow(dead_code, reason = "awaiting the phase-7 prestige flow")]
pub fn render_preview(frame: &mut Frame, area: Rect) {
    super::modal(
        frame,
        area,
        68,
        18,
        " Prestige ",
        &[
            "",
            " Rank  II  →  III            Multiplier  ×1.20  →  ×1.30",
            " Cost  6 540 Amethyst        Held  0 Amethyst          ✗",
            "",
            " You lose                          You keep",
            " ────────────────────────────      ────────────────────────",
            " Diamond Pickaxe → Wooden          Prestige rank",
            " Efficiency IV → 0                 The global multiplier",
            " Fortune III → 0                   Your settings",
            " All 5 enchants → 0",
            " Mining level 23 → 1",
            " Every mine's size and richness",
            " Your entire inventory",
            "",
            " You are Lv 23 of 50 and a Diamond pickaxe short of Netherite —",
            " and Amethyst only drops past the level.",
        ],
    );
}

/// Draws the typed prestige confirm (the real rank comes from the prestige flow,
/// phase 7).
#[allow(dead_code, reason = "awaiting the phase-7 prestige flow")]
pub fn render_confirm(frame: &mut Frame, area: Rect) {
    super::modal(
        frame,
        area,
        56,
        11,
        " Prestige III ",
        &[
            "",
            " This cannot be undone.",
            "",
            " 6 540 Amethyst  →  rank III  (×1.30)",
            " Everything else resets.",
            "",
            " Type  PRESTIGE  to confirm:",
            " > ____________",
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_is_a_two_column_trade_drawn_unaffordable() {
        let frame = crate::overlay::render_to_string(render_preview);
        assert!(
            frame.contains("You lose") && frame.contains("You keep"),
            "{frame}"
        );
        assert!(frame.contains("Diamond Pickaxe → Wooden"), "{frame}");
        assert!(frame.contains("The global multiplier"), "{frame}");
        // Unaffordable, so the honest last line names the progression owed.
        assert!(frame.contains("short of Netherite"), "{frame}");
    }

    #[test]
    fn the_confirm_asks_for_the_typed_word() {
        let frame = crate::overlay::render_to_string(render_confirm);
        assert!(frame.contains("This cannot be undone."), "{frame}");
        assert!(frame.contains("Type  PRESTIGE  to confirm:"), "{frame}");
        assert!(frame.contains("> ____________"), "{frame}");
    }
}
