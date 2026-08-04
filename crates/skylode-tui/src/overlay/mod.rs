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
//!   too small). There is no key that leads there, so they are not modals at all:
//!   the first two are states of [`Session`](crate::session::Session), and the third
//!   is drawn *above* it — a filter over every state, including the title.
//!
//! The [`dev`] menu is a third kind, and it is in neither list: nobody it was built
//! for is a player, and it is compiled out of a release build entirely. It is pulled
//! in the mechanical sense — it captures input and lives in `App::modal` — so it obeys
//! everything §6.2 says about that half. `docs/DEV-MENU.md` is its spec.
//!
//! Both are drawn the same way: [`Clear`] the region first, then render on top.
//! Clearing is what makes an overlay cost **zero permanent layout rows** — it
//! borrows the cells for a frame and the next redraw restores whatever was under it.
//!
//! [`Clear`]: ratatui::widgets::Clear

pub mod compression;
#[cfg(debug_assertions)]
pub mod dev;
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
    style::Style,
    text::Line,
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
};
use skylode_core::material::Material;

use crate::theme;

/// Which way a [`Modal::Compress`] dialog converts.
///
/// The two directions are one dialog and not two, because §6.6 specifies the inverse
/// as *"the same frame with the arithmetic reversed"* — free-and-lossless-both-ways
/// showing up as a UI economy. Carrying the direction as a value rather than as two
/// modal variants is what keeps that literally true: one spinner, one confirm path,
/// one place the numbers are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conversion {
    /// Raw into Compressed units: `c`.
    Compress,
    /// Compressed units back into raw: `C`.
    Decompress,
}

/// A modal overlay that captures input.
///
/// One is still missing ([`settings`]): it is drawn but not yet stacked, and arrives
/// as a variant here when the screen that opens it is wired (phase 9). The exhaustive
/// `match` in [`crate::keymap`] and [`crate::app`] then refuses to compile until it
/// learns to draw and drive.
///
/// **[`Clone`] and no longer [`Copy`]**, since the prestige confirm carries the text
/// the player has typed. A [`String`] owns a heap allocation, and copying one bit for
/// bit would leave two owners of it — so the language refuses `Copy` and the callers
/// ask for a clone instead. The cost is one small allocation per keystroke, on the
/// input path and never on the render one; the alternative was a hand-rolled fixed
/// buffer whose only merit would have been keeping a trait this enum does not need.
///
/// [`settings`]: crate::overlay::settings
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modal {
    /// The full-screen key reference (UI.md §6.11), opened with `?` from any screen.
    Help,
    /// The compression dialog (UI.md §6.6), opened with `c` / `C` from Inventory.
    ///
    /// **The spinner's count lives in the variant, not in a field beside
    /// [`App::modal`](crate::app::App::modal)**, and that is the same device
    /// [`TargetView`](crate::view::TargetView) uses for the cell being dug: two facts
    /// that only mean anything together are stored together, so *"a count of 12 with
    /// no dialog open"* is a state that cannot be written down. The whole payload is
    /// [`Copy`], so [`Modal`] stays `Copy` and nothing downstream changes shape.
    ///
    /// `material` is read from the Inventory cursor at the moment of opening rather
    /// than followed live: the dialog is *about* one pile, and a cursor that could
    /// move underneath it would convert something the player was not looking at.
    Compress {
        /// Which pile is being converted.
        material: Material,
        /// Which way.
        direction: Conversion,
        /// How many units the spinner currently reads, always `1..=max`.
        units: u32,
    },
    /// The tier-jump confirmation (UI.md §6.7), opened by a chain that ends below the
    /// power it started at.
    ///
    /// **The target rung is carried, not re-read from the cursor**, for the reason the
    /// compression dialog carries its material: the modal is *about* one chain, and it
    /// was opened against numbers the player has now read. A cursor cannot move under
    /// a modal today — the modal has every key — but the confirm would silently mean
    /// something else the moment one could.
    ///
    /// `buy` is which of the two options the `▸` is on, and it opens **`false`**: the
    /// dangerous option must not be the one a reflex `Enter` takes. That is §6.7's
    /// prose over its own wireframe, which draws the caret on `Buy it`; the departure
    /// is recorded in `docs/UI.md`.
    Dip {
        /// The ladder index the chain would climb to.
        to: usize,
        /// Whether `Buy it` is the focused option.
        buy: bool,
    },
    /// The prestige preview (UI.md §6.8), opened with `p` from the Stats screen.
    ///
    /// **A unit variant, and the only stateful modal that is one.** The compression
    /// dialog and the dip carry values because a spinner's count and a caret's side
    /// exist nowhere else; this box has nothing of its own to remember. Every figure
    /// in it is [`PrestigeView`](crate::view::PrestigeView)'s, which is the same
    /// projection the Stats panel behind it draws from — so re-reading it per frame is
    /// what *guarantees* the two agree, where a captured copy could go stale against a
    /// tick that credited ore while the box was up.
    PrestigePreview,
    /// The typed prestige confirm (UI.md §6.9), reached by `Enter` on an affordable
    /// preview.
    ///
    /// **The field's contents live here**, for [`Compress`](Modal::Compress)' reason
    /// exactly: two facts that only mean something together are stored together, so
    /// *"half a typed word with no box open"* is a state that cannot be written down.
    /// It is also what makes this the variant that costs the enum its [`Copy`].
    PrestigeConfirm {
        /// What the player has typed so far, verbatim — mistakes included, which is
        /// the whole point of §6.9's argument for typing over a `No / Yes`.
        typed: String,
    },
    /// The dev menu (`docs/DEV-MENU.md`), opened with `` ` `` when the session was
    /// started with `SKYLODE_DEV` set.
    ///
    /// **A unit variant, unlike the two above**, and the departure is deliberate:
    /// those two carry their state so that a value with no dialog open is unwritable,
    /// while this menu's values are *meant* to outlive a close — see
    /// [`DevState`](dev::DevState). They live in `App::dev`, which is also what says
    /// whether the menu exists at all.
    #[cfg(debug_assertions)]
    Dev,
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
        .padding(Padding::horizontal(1))
        .border_style(Style::default().fg(theme::MUTED))
        .title_style(theme::TITLE);
    // The body goes through `marked` line by line: several modals quote an
    // affordability (`Cost … Held … ✗`), and a mark must not change meaning by
    // being drawn inside a box instead of in a list.
    let body: Vec<Line<'static>> = lines.iter().map(|line| theme::marked(line)).collect();
    frame.render_widget(Paragraph::new(body).block(block), rect);
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
        .border_style(Style::default().fg(theme::MUTED))
        .title_style(theme::TITLE)
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
