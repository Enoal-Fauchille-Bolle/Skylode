//! Overlays: things drawn *over* a screen without costing it a row.
//!
//! `docs/UI.md` §6 specifies them one by one, and §2.2 splits them in two kinds
//! that are not the same feature:
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
    text::{Line, Span},
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
/// **[`Clone`] and no longer [`Copy`]**, since the prestige confirm carries the text
/// the player has typed. A [`String`] owns a heap allocation, and copying one bit for
/// bit would leave two owners of it — so the language refuses `Copy` and the callers
/// ask for a clone instead. The cost is one small allocation per keystroke, on the
/// input path and never on the render one; the alternative was a hand-rolled fixed
/// buffer whose only merit would have been keeping a trait this enum does not need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Modal {
    /// The full-screen key reference (UI.md §6.11), opened with `?` from any screen.
    Help,
    /// The Settings screen (UI.md §6.10), opened with `s` from any screen.
    ///
    /// **The selected row lives in the variant**, for the reason
    /// [`Compress`](Modal::Compress) carries its count: *"row three, with no screen
    /// open"* is then a state nobody can write down. It is the caret and **not** the
    /// preferences — those are `App::config`, which is the copy the game is actually
    /// reading and the copy the autosave writes. A row here and a value there is what
    /// keeps this box an editor of the real config rather than of a snapshot that
    /// would have to be merged back.
    ///
    /// It opens on [`ROWS[0]`](settings::ROWS) every time rather than remembering where
    /// it was left, unlike the dev menu: the dev menu's rows *carry dialled values* a
    /// user is mid-way through using, while five preferences fit on one screen and the
    /// top is where a short list opens.
    Settings {
        /// Which of the five rows the `▸` is on.
        row: settings::SettingsRow,
    },
    /// The compression dialog (UI.md §6.6), opened with `c` / `C` from Inventory.
    ///
    /// **The spinner's count lives in the variant, not in a field beside
    /// [`App::modal`](crate::app::App::modal)**, and that is the same device
    /// [`TargetView`](crate::view::TargetView) uses for the cell being dug: two facts
    /// that only mean anything together are stored together, so *"a count of 12 with
    /// no dialog open"* is a state that cannot be written down. This payload is
    /// [`Copy`], but [`Modal`] itself is not: [`PrestigeConfirm`](Modal::PrestigeConfirm)
    /// owns the [`String`] the player types, so the enum is [`Clone`] and call sites
    /// take it by reference or clone it deliberately.
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
    modal_with_hint(frame, area, width, height, title, lines, None);
}

/// [`modal`], plus a key hint drawn **muted** as the box's last line.
///
/// **Muted, like every footer in the crate**, and that is the whole reason the hint is
/// a parameter instead of another string in `lines`: `mine`'s footer says it outright
/// — *"a key hint is the least urgent thing on the screen, and the whole point of
/// giving the chrome a de-emphasised colour is that the rows above it can then be read
/// without competition"*. Passed inside `lines` it went through [`theme::marked`] like
/// every other row, found no mark in `Enter  collect`, and came out drawn exactly as
/// loud as the numbers it sits under.
///
/// One definition for what a hint looks like, in the same function that already owns
/// what a *box* looks like — so the offline summary, the compression dialog and the
/// dev menu cannot drift apart on it.
///
/// **The caller still owns the height.** The hint is one more row inside the box, so a
/// caller deriving its height from `lines.len()` has to count it; the two callers that
/// pass a fixed height already have the room.
pub(super) fn modal_with_hint(
    frame: &mut Frame,
    area: Rect,
    width: u16,
    height: u16,
    title: &str,
    lines: &[&str],
    hint: Option<&str>,
) {
    // The body goes through `marked` line by line: several modals quote an
    // affordability (`Cost … Held … ✗`), and a mark must not change meaning by
    // being drawn inside a box instead of in a list.
    let body = lines.iter().map(|line| theme::marked(line)).collect();
    modal_lines(frame, area, width, height, title, body, hint);
}

/// [`modal_with_hint`] over a body the caller has **already styled**.
///
/// The two wrappers above are the common case — hand over strings, get `theme::marked`
/// applied for you — and this is the seam for the one box whose rows carry a hierarchy
/// that a mark scan cannot express: `save_recovery` mutes each choice's consequence
/// column through [`theme::marked_tail`].
///
/// **This does not loosen "one place decides what a box looks like".** What a caller
/// gains here is its own *body*, which was always its content; the border, the title,
/// the inset and the hint are still decided in exactly one function, so the compression
/// dialog and the dip cannot drift apart on any of them.
pub(super) fn modal_lines(
    frame: &mut Frame,
    area: Rect,
    width: u16,
    height: u16,
    title: &str,
    mut body: Vec<Line<'static>>,
    hint: Option<&str>,
) {
    let rect = centered_rect(area, width, height);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_type(BorderType::Plain)
        .title(title.to_owned())
        .padding(Padding::horizontal(1))
        .border_style(Style::default().fg(theme::MUTED))
        .title_style(theme::TITLE);
    // The hint is chrome, so it takes the muted hue whole rather than being scanned
    // for marks it does not carry.
    if let Some(hint) = hint {
        body.push(Line::from(Span::styled(
            hint.to_owned(),
            Style::default().fg(theme::MUTED),
        )));
    }
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
///
/// 80×24 is the counted frame, so this is the size almost every assertion wants.
/// [`render_to_string_sized`] is for the few that are *about* the size.
#[cfg(test)]
pub(super) fn render_to_string(draw: impl FnOnce(&mut Frame, Rect)) -> String {
    render_to_string_sized(80, 24, draw)
}

/// [`render_to_string`] at a chosen size.
///
/// Split out for the overlays whose behaviour is a function of the window: the title
/// centres its block in the slack, so proving it moved needs a frame with slack in it,
/// and a test that only ever drew 80×24 would pass on a layout pinned to the top.
#[cfg(test)]
pub(super) fn render_to_string_sized(
    width: u16,
    height: u16,
    draw: impl FnOnce(&mut Frame, Rect),
) -> String {
    let buffer = render_to_buffer(width, height, draw);
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The same draw, handed back as **cells** rather than as text.
///
/// The two harnesses are one draw and two readings, which is what keeps a colour test
/// and a text test from disagreeing about what was on screen. Text answers *"does it
/// say this"*; this answers *"in what hue"*, and the conformance rules this crate
/// applies to its chrome — muted footers, accented carets — are only assertable
/// through the second.
#[cfg(test)]
pub(super) fn render_to_buffer(
    width: u16,
    height: u16,
    draw: impl FnOnce(&mut Frame, Rect),
) -> ratatui::buffer::Buffer {
    use ratatui::{Terminal, backend::TestBackend};

    let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
        Ok(terminal) => terminal,
        Err(infallible) => match infallible {},
    };
    if let Err(infallible) = terminal.draw(|frame| {
        let area = frame.area();
        draw(frame, area);
    }) {
        match infallible {}
    }
    terminal.backend().buffer().clone()
}

/// The foreground colour of the first cell whose symbol is `needle`.
///
/// Scans row by row, so "the caret" and "the first border corner" are one call each
/// rather than a coordinate a layout change would invalidate.
#[cfg(test)]
pub(super) fn colour_of(
    buffer: &ratatui::buffer::Buffer,
    needle: &str,
) -> Option<ratatui::style::Color> {
    (0..buffer.area.height).find_map(|y| {
        (0..buffer.area.width)
            .find(|&x| buffer[(x, y)].symbol() == needle)
            .map(|x| buffer[(x, y)].fg)
    })
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

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

    /// The defect `modal_with_hint` exists for: a key hint drawn exactly as loud as
    /// the figures above it.
    ///
    /// Both halves are asserted in one draw, because the claim is a *contrast* — a
    /// hint that muted the whole box with it would pass a test that only looked at the
    /// hint.
    #[test]
    fn a_modal_hint_is_muted_and_the_body_above_it_is_not() {
        let buffer = render_to_buffer(60, 12, |frame, area| {
            modal_with_hint(
                frame,
                area,
                40,
                8,
                " Title ",
                &["", " Blocks    76 / 84"],
                Some(" Enter  collect"),
            );
        });

        // `7` is only in the body figure, `E` only in the hint. `Color::Reset` is what
        // an unstyled cell holds — the terminal's own foreground, which is exactly what
        // a body row is supposed to keep.
        assert_eq!(colour_of(&buffer, "7"), Some(Color::Reset));
        assert_eq!(
            colour_of(&buffer, "E"),
            Some(theme::MUTED),
            "the hint was drawn as loud as the numbers above it"
        );
    }

    /// A modal without a hint must be byte-for-byte what it was before the parameter
    /// existed — six call sites pass `None` through `modal`, and none of them may
    /// change appearance.
    #[test]
    fn a_modal_with_no_hint_draws_what_it_always_drew() {
        let draw = |hinted: bool| {
            render_to_string_sized(60, 12, move |frame, area| {
                if hinted {
                    modal_with_hint(frame, area, 40, 8, " Title ", &["", " Body"], None);
                } else {
                    modal(frame, area, 40, 8, " Title ", &["", " Body"]);
                }
            })
        };
        assert_eq!(draw(true), draw(false));
    }

    #[test]
    fn centring_respects_a_non_zero_origin() {
        let area = Rect::new(10, 5, 20, 10);
        let rect = centered_rect(area, 10, 4);
        assert_eq!(rect, Rect::new(15, 8, 10, 4));
    }
}
