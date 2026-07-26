//! The Settings screen (UI.md §6.10).
//!
//! **Every config field, and no game-state field.** That rule is only auditable if
//! the list is short enough to read at a glance, and this frame is what makes the
//! audit possible: every line is a preference and nothing here is state. A setting
//! is what you add when a preference is genuinely contested — not a way of declining
//! to decide. Stored in the save, edited only here, which is what keeps the HMAC
//! quiet when a colour changes.
//!
//! Square borders (`┌┐`), the boot-and-modal class, not the screens' rounded ones.
//! `#[allow(dead_code)]` until it is reachable from the splash and the game (phase 7).

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::Paragraph,
};

use super::square;

/// Draws the Settings screen with placeholder values (real config is save state,
/// phase 7).
#[allow(dead_code, reason = "awaiting the phase-7 settings binding")]
pub fn render(frame: &mut Frame, area: Rect) {
    let [body, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let [left, right] =
        Layout::horizontal([Constraint::Length(38), Constraint::Min(0)]).areas(body);

    let fields = [
        " ▸ Colour            256",
        "   Mining input      Hold",
        "   Number format     1 234 567",
        "   Toast duration    3s",
        "   Sub-tab keys      ⇧← ⇧→",
    ];
    frame.render_widget(
        Paragraph::new(fields.join("\n")).block(square(" Settings ")),
        left,
    );

    let detail = [
        " Colour",
        "",
        " 256   every block gets its own swatch",
        " 16    one colour per mine; the value",
        "       cell's stipple is what still",
        "       tells it from the common one",
        "",
        " Your terminal reports: 256 supported",
        "",
        " Stored in the save. There is no config",
        " file; Settings is the only way to",
        " change these — which is what keeps the",
        " HMAC quiet when you change a colour.",
    ];
    frame.render_widget(Paragraph::new(detail.join("\n")).block(square("")), right);

    frame.render_widget(
        Paragraph::new(" ↑↓  select     ← →  change     Esc  back"),
        footer_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_lists_only_preferences_with_the_colour_detail() {
        let frame = crate::overlay::render_to_string(render);
        assert!(frame.contains("Colour            256"), "{frame}");
        assert!(frame.contains("Mining input      Hold"), "{frame}");
        assert!(frame.contains("Sub-tab keys"), "{frame}");
        // The detail pane explains why editing here does not upset the HMAC.
        assert!(frame.contains("keeps the"), "{frame}");
        assert!(frame.contains("Esc  back"), "{frame}");
    }
}
