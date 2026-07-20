//! The terminal-too-small filter (UI-EN.md §5.7.2, §6.2).
//!
//! This is **not** a screen and **not** a [`super::Modal`]. It has no incoming key
//! and no outgoing one: it is a pure function of the frame's size, drawn in front
//! of the renderer when the terminal is below the 80×24 budget every frame is
//! counted against. It interrupts whatever was on screen — a tab, a stacked modal,
//! anything — and returns to it untouched the moment the window grows, because it
//! never reads or writes the model. Drawing it as a state with edges would be a
//! lie; it is a filter.
//!
//! It is also the one screen in the game with **no minimum**: it must render at
//! *any* size, so below the box's own footprint it degrades to the bare `80 x 24`
//! target with no border at all.
//!
//! Per the bootstrap rule (UI-EN.md §3.4) it renders with **hardcoded defaults**
//! only — no [`crate::view::View`], no config. There is nothing here a save could
//! colour, which is the rule doing its job rather than being fought.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    widgets::{Block, BorderType, Clear, Padding, Paragraph},
};

use super::centered_rect;

/// The reference budget every frame in UI-EN.md is counted against.
///
/// Public because [`fits`] is the predicate the render loop gates on, and a caller
/// that wants to explain *why* it is showing this screen reads the same two
/// numbers the box prints.
pub const MIN_WIDTH: u16 = 80;
/// The reference height. See [`MIN_WIDTH`].
pub const MIN_HEIGHT: u16 = 24;

/// The message box's footprint. Below this the box itself will not fit, so the
/// filter drops to the bare target — the "no minimum" clause of §5.7.2.
const BOX_WIDTH: u16 = 44;
/// The message box's height. See [`BOX_WIDTH`].
const BOX_HEIGHT: u16 = 10;

/// Whether `area` meets the 80×24 budget.
///
/// The one predicate the render loop consults: `true` lets the normal frame draw,
/// `false` routes to [`render`] instead. Split out from the drawing so the rule —
/// *both* dimensions must be met, not either — is a plain unit test rather than
/// something only observable through a rendered buffer.
pub fn fits(area: Rect) -> bool {
    area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT
}

/// Draws the too-small screen over the whole frame.
///
/// Clears `area` first: the loop reaches here by *skipping* the normal render, so
/// without a clear the previous frame's screen would show around the centred box.
/// ratatui does not blank the back buffer between draws, so this is the overlay
/// convention (clear the region, then paint) applied to the whole frame.
pub fn render(frame: &mut Frame, area: Rect) {
    frame.render_widget(Clear, area);

    // The one screen with no minimum: below the box's own size, even the border
    // goes and only the target remains, centred. `centered_rect` clamps, so this
    // branch is about dropping the chrome, not about avoiding an overflow.
    if area.width < BOX_WIDTH || area.height < BOX_HEIGHT {
        let line = centered_rect(area, BOX_WIDTH.min(area.width), 1);
        let target =
            Paragraph::new(format!("{MIN_WIDTH} x {MIN_HEIGHT}")).alignment(Alignment::Center);
        frame.render_widget(target, line);
        return;
    }

    let lines = [
        String::new(),
        format!("Skylode needs {MIN_WIDTH} x {MIN_HEIGHT}"),
        String::new(),
        format!("This terminal is {} x {}", area.width, area.height),
        underline(area),
        String::new(),
        "Enlarge the window, or press q to quit.".to_owned(),
    ];

    // Square corners, not the rounded ones the placeholders use: the boot screens
    // are visually a class apart, and the wireframe draws them `┌┐└┘`. Horizontal
    // padding of 1 is the `│ Skylode` inset — and because the text line and the
    // caret line below both pass through it, their alignment is preserved.
    let block = Block::bordered()
        .border_type(BorderType::Plain)
        .padding(Padding::horizontal(1));
    let rect = centered_rect(area, BOX_WIDTH, BOX_HEIGHT);
    frame.render_widget(Paragraph::new(lines.join("\n")).block(block), rect);
}

/// Builds the caret row that underlines whichever dimension is too small.
///
/// The point of the underline (§5.7.2): a player at 80×18 must learn it is the
/// *height* that is short, not the width. So the carets sit under the offending
/// number only — under both, and the ` x ` between them, when both are short,
/// which is what makes the wireframe's continuous `^^^^^^^` appear.
fn underline(area: Rect) -> String {
    const PREFIX: &str = "This terminal is ";
    const SEP: &str = " x ";

    let width_str = area.width.to_string();
    let height_str = area.height.to_string();
    let width_bad = area.width < MIN_WIDTH;
    let height_bad = area.height < MIN_HEIGHT;

    let marks = if width_bad && height_bad {
        "^".repeat(width_str.len() + SEP.len() + height_str.len())
    } else {
        let width_marks = if width_bad { "^" } else { " " }.repeat(width_str.len());
        let sep_gap = " ".repeat(SEP.len());
        let height_marks = if height_bad { "^" } else { " " }.repeat(height_str.len());
        format!("{width_marks}{sep_gap}{height_marks}")
    };

    format!("{}{}", " ".repeat(PREFIX.len()), marks)
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use super::*;

    /// Draws the filter into an off-screen `width × height` terminal.
    ///
    /// Same trick as `app`'s render test: `TestBackend` writes into a `Vec` of
    /// cells, whose operations are `Result<_, Infallible>`, so the errors are
    /// discharged with an empty `match` on the uninhabited error rather than an
    /// `unwrap` the lints would flag.
    fn render_to_buffer(width: u16, height: u16) -> Buffer {
        let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        if let Err(infallible) = terminal.draw(|frame| render(frame, frame.area())) {
            match infallible {}
        }
        terminal.backend().buffer().clone()
    }

    /// The whole frame joined into one string, for "is this text on screen".
    fn whole_frame(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_exact_budget_fits_but_one_short_of_either_axis_does_not() {
        assert!(fits(Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT)));
        assert!(!fits(Rect::new(0, 0, MIN_WIDTH - 1, MIN_HEIGHT)));
        assert!(!fits(Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT - 1)));
    }

    #[test]
    fn the_box_states_the_target_the_actual_size_and_the_quit_hint() {
        let frame = whole_frame(&render_to_buffer(54, 18));
        assert!(frame.contains("Skylode needs 80 x 24"), "{frame}");
        assert!(frame.contains("This terminal is 54 x 18"), "{frame}");
        assert!(frame.contains("press q to quit"), "{frame}");
    }

    #[test]
    fn a_short_height_underlines_the_height_and_not_the_width() {
        // Width 80 is fine, height 18 is not: only the second number is flagged.
        let underline = underline(Rect::new(0, 0, 80, 18));
        assert!(
            underline.ends_with("^^"),
            "expected carets under 18: {underline:?}"
        );
        // The width number "80" sits right after the 17-char prefix; it must be
        // blank, or the player is told the wrong axis is at fault.
        assert_eq!(
            &underline[17..19],
            "  ",
            "width should not be flagged: {underline:?}"
        );
    }

    #[test]
    fn both_axes_short_underlines_across_the_whole_reading() {
        // Matches the wireframe's continuous `^^^^^^^` under `54 x 18`.
        let underline = underline(Rect::new(0, 0, 54, 18));
        assert!(underline.trim_start().chars().all(|c| c == '^'));
        assert_eq!(underline.trim_start().chars().count(), "54 x 18".len());
    }

    #[test]
    fn below_the_box_footprint_only_the_bare_target_remains() {
        let buffer = render_to_buffer(20, 6);
        let frame = whole_frame(&buffer);
        assert!(frame.contains("80 x 24"), "{frame}");
        // No border: the box is gone, so no corner glyph survives.
        assert!(
            !frame.contains('┌'),
            "the box should be dropped at this size: {frame}"
        );
    }
}
