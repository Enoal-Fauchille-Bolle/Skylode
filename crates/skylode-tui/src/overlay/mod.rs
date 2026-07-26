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

pub mod compression;
pub mod dip;
pub mod help;
pub mod offline;
pub mod prestige;
pub mod save_recovery;
pub mod settings;
pub mod splash;
pub mod too_small;

use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
};

/// A modal overlay that captures input.
///
/// One variant so far — [`Help`](Modal::Help), the first pulled overlay to be
/// reachable. The others ([`compression`], [`dip`], [`prestige`], [`settings`]) are
/// drawn but not yet stacked: they arrive as variants here when the screens that
/// open them are wired (phases 5–7), and the exhaustive `match` in [`crate::keymap`]
/// and [`crate::app`] then refuses to compile until each learns to draw and drive.
///
/// [`compression`]: crate::overlay::compression
/// [`dip`]: crate::overlay::dip
/// [`prestige`]: crate::overlay::prestige
/// [`settings`]: crate::overlay::settings
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modal {
    /// The full-screen key reference (UI.md §6.11), opened with `?` from any screen.
    Help,
}

/// Draws a centred modal box of `width × height`, titled, filled with `lines`.
///
/// The overlay convention in one call: [`Clear`] the region so the screen behind
/// does not show through, then a **square-bordered** box — the boot-and-modal class
/// is `┌┐└┘`, set apart from the screens' rounded `╭╮` — with a one-column inset so
/// the text never touches the border. Every modal in the design goes through here,
/// so the frame's chrome cannot drift between the compression dialog and the dip.
///
/// [`Clear`]: ratatui::widgets::Clear
pub(super) fn modal(
    frame: &mut Frame,
    area: Rect,
    width: u16,
    height: u16,
    title: &str,
    lines: &[&str],
) {
    let rect = centered_rect(area, width, height);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Plain)
        .title(title.to_owned())
        .padding(Padding::horizontal(1));
    frame.render_widget(Paragraph::new(lines.join("\n")).block(block), rect);
}

/// A square-bordered panel titled `title` — the boot-and-modal class's box, set
/// apart from the screens' rounded [`crate::screen::panel`]. Shared by the two
/// full-screen overlays (Settings, Help) that split into panels rather than centre
/// one box.
pub(super) fn square(title: &str) -> Block<'static> {
    Block::default()
        .title(title.to_owned())
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
}

/// Centres a `width × height` box inside `area`.
///
/// Takes absolute dimensions rather than percentages because every modal in the
/// design is specified at a counted size (the compression dialog is 48×11, the
/// dip modal 64×13): a percentage would re-derive a number the wireframe already
/// settled, and would drift as the terminal grows. Both dimensions are clamped,
/// so an oversized box on a small terminal is truncated rather than overflowing.
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

/// Renders an overlay's `draw` into an 80×24 buffer and returns the whole frame
/// as one string — the shared harness every overlay's snapshot test draws through,
/// so each module asserts on content rather than re-spelling `TestBackend` setup.
#[cfg(test)]
pub(super) fn render_to_string(draw: impl FnOnce(&mut Frame, Rect)) -> String {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal = match Terminal::new(TestBackend::new(80, 24)) {
        Ok(terminal) => terminal,
        Err(infallible) => match infallible {},
    };
    if let Err(infallible) = terminal.draw(|frame| {
        let area = frame.area();
        draw(frame, area);
    }) {
        match infallible {}
    }
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
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
