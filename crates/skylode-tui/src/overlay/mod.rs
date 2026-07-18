//! Overlays: things drawn *over* a screen without costing it a row.
//!
//! UI-EN.md §5.7 counts ten of them, and §6.2 splits them in two kinds that are
//! not the same feature:
//!
//! - **Pulled** — the player opens them (compression dialog, dip modal, prestige
//!   preview/confirm, settings). They capture input, so they live in
//!   [`crate::app::App::modal`] and [`crate::keymap::resolve`] gives them first
//!   refusal on every key.
//! - **Pushed** — the game raises them (offline summary, save recovery, terminal
//!   too small). There is no key that leads there, so they are not modals at all;
//!   they belong to the session state machine, which lands in a later pass.
//!
//! Both are drawn the same way: [`Clear`] the region first, then render on top.
//! Clearing is what makes an overlay cost **zero permanent layout rows** — it
//! borrows the cells for a frame and the next redraw restores whatever was under it.
//!
//! [`Clear`]: ratatui::widgets::Clear

use ratatui::layout::Rect;

/// A modal overlay that captures input.
///
/// **Uninhabited on purpose.** There are no modals yet, and an enum with zero
/// variants says exactly that: the slot in [`crate::app::App`] exists and is
/// wired through the keymap, but no value can ever be constructed, so
/// `Option<Modal>` is provably always `None` and every `match` on it is empty.
/// When the compression dialog arrives it becomes a variant here and the
/// compiler points at each place that must learn to draw and drive it.
#[derive(Clone, Copy, Debug)]
pub enum Modal {}

/// Centres a `width × height` box inside `area`.
///
/// Takes absolute dimensions rather than percentages because every modal in the
/// design is specified at a counted size (the compression dialog is 48×11, the
/// dip modal 64×13): a percentage would re-derive a number the wireframe already
/// settled, and would drift as the terminal grows. Both dimensions are clamped,
/// so an oversized box on a small terminal is truncated rather than overflowing.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "consumed by the modal overlays in a later pass; see UI-EN.md §5.7"
    )
)]
pub fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_box_is_centred_within_its_area() {
        let area = Rect::new(0, 0, 80, 24);
        let rect = centered_rect(area, 48, 12);
        assert_eq!(rect, Rect::new(16, 6, 48, 12));
    }

    #[test]
    fn a_box_larger_than_its_area_is_clamped_rather_than_overflowing() {
        let area = Rect::new(0, 0, 40, 10);
        let rect = centered_rect(area, 64, 13);
        assert_eq!(rect, area);
    }

    #[test]
    fn centring_respects_a_non_zero_origin() {
        let area = Rect::new(10, 5, 20, 10);
        let rect = centered_rect(area, 10, 4);
        assert_eq!(rect, Rect::new(15, 8, 10, 4));
    }
}
