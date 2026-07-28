//! The Inventory screen — ores held, and manual compression (UI.md §5.3).
//!
//! A fifteen-row table of materials in both denominations on the left, a Compress
//! panel for the selected one on the right. The table fits without a scrollbar
//! only because the material list is **closed at fifteen** and numbers are exact
//! with separators rather than columns of `1.23M`.
//!
//! The frame is drawn **mid-refusal on purpose**: Iron is worth 680 and an upgrade
//! costs 650, and the player still cannot buy it, because costs are paid in the
//! denomination they are quoted in. So the panel names the *missing denomination*
//! and shows `Compressible now` — a screen that only said "you cannot afford this"
//! would be lying, since the player can. The refusal text is a placeholder until
//! the three-state affordability query lands (phase 5); the derived numbers here —
//! value, compressible-now — are computed from what the player holds.

use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::Paragraph,
};
use skylode_core::tunables::RAW_PER_COMPRESSED;

use crate::{action::Action, format::grouped, screen::panel, theme, view::View};

/// The table's share of the row, against [`COMPRESS_WEIGHT`] — the counted widths
/// reused as `Fill` weights, per the module note on `screen`.
const TABLE_WEIGHT: u16 = 48;

/// The Compress panel's share — the 32 columns UI-EN.md §5.4 counted.
const COMPRESS_WEIGHT: u16 = 32;

/// Draws the table, the compress panel, and the footer.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    let [body, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    let [table_area, compress_area] = Layout::horizontal([
        Constraint::Fill(TABLE_WEIGHT),
        Constraint::Fill(COMPRESS_WEIGHT),
    ])
    .areas(body);
    table(frame, table_area, view);
    compress(frame, compress_area, view);

    let footer = " ↑↓  select     c  compress     C  decompress     Tab  next screen";
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme::MUTED)),
        footer_area,
    );
}

/// The material table: a header, then one row per material, the cursor marked `▸`.
fn table(frame: &mut Frame, area: Rect, view: &View) {
    let block = panel(" Inventory ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        Line::from(row("   ", "Material", "Compressed", "Raw"))
            .style(Style::default().fg(theme::MUTED)),
    ];
    for (index, item) in view.inventory.rows.iter().enumerate() {
        // The cursor is a mark, not a highlight style, so it survives a colourless
        // terminal and reads the same way the `▸` on Levels and Stats does. Its
        // colour is derived from that same glyph, so the two cannot disagree.
        let mark = if index == view.inventory.selected {
            " ▸ "
        } else {
            "   "
        };
        lines.push(theme::marked(&row(
            mark,
            &item.material,
            &grouped(item.compressed),
            &grouped(item.raw),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// One table row: a three-column mark, a left material, two right-aligned counts.
///
/// The header passes plain words through the same widths as the numbers, so its
/// `Compressed`/`Raw` labels sit right-aligned over the columns they name.
fn row(mark: &str, material: &str, compressed: &str, raw: &str) -> String {
    format!("{mark}{material:<16}{compressed:>12}{raw:>12}")
}

/// The Compress panel: the selected material in both denominations, the two
/// conversions, and the compress-first context.
fn compress(frame: &mut Frame, area: Rect, view: &View) {
    let block = panel(" Compress ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Total by construction: `selected` is a validated index in the fixture, but a
    // `get` keeps a future off-by-one from panicking a renderer over a cursor.
    let Some(item) = view.inventory.rows.get(view.inventory.selected) else {
        return;
    };
    // Both derived from what is held: the value in the common denomination, and how
    // many compressed units the raw pile could still mint.
    let value = item.raw + item.compressed * RAW_PER_COMPRESSED;
    let compressible = item.raw / RAW_PER_COMPRESSED;

    let mut lines = vec![
        Line::from(format!(" {}", item.material)),
        Line::from(""),
        Line::from(format!(" Held     {} Compressed", grouped(item.compressed))),
        Line::from(format!("          {} Raw", grouped(item.raw))),
        Line::from(format!(" Value    {} {}", grouped(value), item.material)),
        Line::from(""),
        Line::from(format!(" c   compress  {RAW_PER_COMPRESSED} raw → 1")),
        Line::from(format!(" C   decompress  1 → {RAW_PER_COMPRESSED} raw")),
        Line::from(""),
        Line::from(format!(" Compressible now:  {}", grouped(compressible))),
        Line::from(""),
    ];
    // The compress-first context, pre-wrapped in the fixture.
    for line in &view.inventory.hint {
        lines.push(Line::from(format!(" {line}")));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(" Free and lossless both ways."));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// No contextual bindings yet; `c` / `C` arrive with the compression dialog.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    use super::*;

    /// Renders the Inventory screen alone into an 80×24 buffer.
    fn render_screen() -> Buffer {
        let view = View::sample();
        let mut terminal = match Terminal::new(TestBackend::new(80, 24)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        if let Err(infallible) = terminal.draw(|frame| {
            let area = frame.area();
            render(frame, area, &view);
        }) {
            match infallible {}
        }
        terminal.backend().buffer().clone()
    }

    /// Every row joined — for "is this text on screen anywhere".
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

    /// The one row that contains `needle`, or the whole frame on failure.
    fn row_with<'a>(frame: &'a str, needle: &str) -> &'a str {
        frame
            .lines()
            .find(|line| line.contains(needle))
            .unwrap_or(frame)
    }

    /// Just the table's columns (0..48), joined per row.
    ///
    /// The panels sit side by side, so `Iron` shows in both the table row and the
    /// Compress panel on the same terminal line; slicing the table off is what lets
    /// a test name the table row without matching the panel title beside it.
    fn table_panel(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| (0..48).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_table_has_its_header_and_the_fifteen_materials() {
        let frame = whole_frame(&render_screen());
        assert!(
            row_with(&frame, "Material").contains("Compressed"),
            "{frame}"
        );
        assert!(row_with(&frame, "Material").contains("Raw"), "{frame}");
        // First and last of the closed fifteen, with a grouped count between them.
        assert!(row_with(&frame, "Stone").contains("4 508"), "{frame}");
        assert!(frame.contains("Amethyst"), "{frame}");
    }

    #[test]
    fn the_cursor_marks_the_selected_row() {
        let table = table_panel(&render_screen());
        // Iron is the selected row; its line carries the `▸` and its raw count.
        let iron = row_with(&table, "Iron");
        assert!(
            iron.contains('▸'),
            "selected row lacks the cursor: {iron:?}"
        );
        assert!(iron.contains("480"), "{iron:?}");
        // A different material's row must not also carry the cursor.
        assert!(!row_with(&table, "Stone").contains('▸'), "{table}");
    }

    #[test]
    fn the_compress_panel_details_the_selected_material() {
        let frame = whole_frame(&render_screen());
        assert!(frame.contains("Held     2 Compressed"), "{frame}");
        assert!(frame.contains("480 Raw"), "{frame}");
        // Value = 480 + 2 × 100 = 680, in the common denomination.
        assert!(frame.contains("Value    680 Iron"), "{frame}");
        // Compressible now = 480 / 100 = 4, derived from the raw pile.
        assert!(frame.contains("Compressible now:  4"), "{frame}");
    }

    #[test]
    fn the_panel_names_the_missing_denomination_not_a_shortfall() {
        let frame = whole_frame(&render_screen());
        // The compress-first refusal, spelled as which denomination is short.
        assert!(frame.contains("Efficiency V wants"), "{frame}");
        assert!(frame.contains("You hold the value, not"), "{frame}");
        assert!(frame.contains("the denomination."), "{frame}");
    }

    #[test]
    fn the_footer_names_select_and_both_conversions() {
        let buffer = render_screen();
        let last = (0..buffer.area.width)
            .map(|x| buffer[(x, 23)].symbol())
            .collect::<String>();
        assert!(last.contains("↑↓  select"), "{last:?}");
        assert!(last.contains("c  compress"), "{last:?}");
        assert!(last.contains("C  decompress"), "{last:?}");
    }

    /// The foreground of the first cell drawn with `glyph`. See the same helper on
    /// the Mines screen for why the lookup goes through the glyph.
    fn fg_of(buffer: &Buffer, glyph: &str) -> Option<Color> {
        buffer
            .content()
            .iter()
            .find(|cell| cell.symbol() == glyph)
            .map(|cell| cell.fg)
    }

    #[test]
    fn the_cursor_keeps_its_glyph_and_takes_the_accent() {
        // The table's cursor is a mark and not a highlighted row, so it has to
        // survive a colourless terminal — the glyph half of this assertion is what
        // pins that it still does.
        let buffer = render_screen();
        assert_eq!(fg_of(&buffer, "▸"), Some(theme::ACCENT));
    }

    #[test]
    fn a_cursor_past_the_last_row_empties_the_panel_instead_of_panicking() {
        // The `get` guard in `compress`. Indexing would panic the *renderer* over a
        // stale cursor — the frame after a row is removed, say — and a panic in a
        // draw call takes the terminal down mid-frame, so the guard returns early and
        // leaves the panel blank rather than half-drawn.
        //
        // The screen still renders: the box, its title and the table beside it are
        // all there, and only the panel's contents are missing. That is the whole
        // claim, so both halves are asserted — a guard that blanked the screen would
        // pass a test that only checked for the absence of a panic.
        let mut view = View::sample();
        view.inventory.selected = view.inventory.rows.len();

        let mut terminal = match Terminal::new(TestBackend::new(80, 24)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        if let Err(infallible) = terminal.draw(|frame| {
            let area = frame.area();
            render(frame, area, &view);
        }) {
            match infallible {}
        }
        let frame = whole_frame(terminal.backend().buffer());

        assert!(
            frame.contains("Compress"),
            "the panel's box is gone: {frame}"
        );
        assert!(
            !frame.contains("Free and lossless"),
            "the panel drew contents for a row that does not exist: {frame}"
        );
    }
}
