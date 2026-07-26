//! The dip modal (UI.md §6.7).
//!
//! Fires **only on a net regression** — a pickaxe chain that crosses a tier jump
//! and ends below the power it started at — never on an ordinary Efficiency step, or
//! it would be a modal nobody reads. It is **not a warning**: the dip is a chosen
//! decision point, so the frame states the trade in ticks per block (a number the
//! player can feel, unlike raw power) and offers the deal, `Not yet` the default.
//!
//! `#[allow(dead_code)]`: live in tests, dead in the bin until the Upgrades screen
//! confirms a tier jump through it (phase 6).

use ratatui::{Frame, layout::Rect};

/// Draws the dip modal with placeholder numbers (the real dip is `Pickaxe`'s, phase 6).
#[allow(dead_code, reason = "awaiting the phase-6 tier-jump confirm")]
pub fn render(frame: &mut Frame, area: Rect) {
    super::modal(
        frame,
        area,
        64,
        13,
        " Netherite Pickaxe ",
        &[
            "",
            " Buying Diamond Efficiency V, then the tier jump.",
            " This resets Efficiency V to 0.",
            "",
            " Mining power      34.0   →   9.0",
            " Ancient Debris    27 ticks  →  100 ticks per block",
            "",
            " You get it back at Netherite Efficiency V (35.0),",
            " five purchases later.",
            "",
            " ▸  Buy it       n  Not yet",
        ],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_states_the_trade_in_ticks_and_offers_the_deal() {
        let frame = crate::overlay::render_to_string(render);
        assert!(frame.contains("This resets Efficiency V to 0."), "{frame}");
        // The dip in ticks per block, not only in power.
        assert!(
            frame.contains("Mining power      34.0   →   9.0"),
            "{frame}"
        );
        assert!(
            frame.contains("27 ticks  →  100 ticks per block"),
            "{frame}"
        );
        assert!(frame.contains("▸  Buy it"), "{frame}");
        assert!(frame.contains("n  Not yet"), "{frame}");
    }
}
