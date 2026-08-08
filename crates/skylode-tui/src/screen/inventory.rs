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
//! would be lying, since the player can.
//!
//! That refusal half is an [`Option`](crate::view::InventoryView::hint), filled only
//! when `Enter` on the Upgrades screen has actually produced a `CompressFirst`
//! verdict **for this material** — so a fresh run draws no refusal block at all, and a
//! remembered one follows the cursor off its own row. Everything else here is real — the
//! counts come from the run's [`Inventory`](skylode_core::inventory::Inventory), and
//! the two derived numbers (value, compressible-now) are computed from what the
//! player holds.

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
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
        Line::from(row("   ", "Material", "Raw", "Compressed"))
            .style(Style::default().fg(theme::MUTED)),
    ];
    for item in &view.inventory.rows {
        // The cursor is a mark, not a highlight style, so it survives a colourless
        // terminal and reads the same way the `▸` on Levels and Stats does. Its
        // colour is derived from that same glyph, so the two cannot disagree.
        //
        // Compared as a `Material` and not as a row index: the cursor is typed, so
        // "which row is selected" is one fact read twice rather than two numbers
        // that can disagree.
        let mark = if item.material == view.inventory.selected {
            " ▸ "
        } else {
            "   "
        };
        lines.push(theme::marked(&row(
            mark,
            item.material.name(),
            &grouped(item.raw),
            &grouped(item.compressed),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// One table row: a three-column mark, a left material, two right-aligned counts.
///
/// The header passes plain words through the same widths as the numbers, so its
/// `Raw`/`Compressed` labels sit right-aligned over the columns they name.
fn row(mark: &str, material: &str, raw: &str, compressed: &str) -> String {
    format!("{mark}{material:<16}{raw:>12}{compressed:>12}")
}

/// The Compress panel: the selected material in both denominations, the two
/// conversions, and the compress-first context.
fn compress(frame: &mut Frame, area: Rect, view: &View) {
    let block = panel(" Compress ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // A row is found rather than indexed: the cursor is a `Material`, so the lookup
    // is what keeps it honest. `None` is unreachable while the table lists all
    // fifteen, and the early return is here because a panic in a draw call takes the
    // terminal down mid-frame.
    let Some(item) = view
        .inventory
        .rows
        .iter()
        .find(|row| row.material == view.inventory.selected)
    else {
        return;
    };
    let name = item.material.name();
    // Both derived from what is held: the value in the common denomination, and how
    // many compressed units the raw pile could still mint.
    let value = item.raw + item.compressed * RAW_PER_COMPRESSED;
    let compressible = item.raw / RAW_PER_COMPRESSED;

    let mut lines = vec![
        Line::from(format!(" {name}")),
        Line::from(""),
        Line::from(format!(" Held     {} Raw", grouped(item.raw))),
        Line::from(format!("          {} Compressed", grouped(item.compressed))),
        Line::from(format!(" Value    {} {name}", grouped(value))),
        Line::from(""),
        Line::from(format!(" c   compress  {RAW_PER_COMPRESSED} raw → 1")),
        Line::from(format!(" C   decompress  1 → {RAW_PER_COMPRESSED} raw")),
        Line::from(""),
        Line::from(format!(" Compressible now:  {}", grouped(compressible))),
        Line::from(""),
    ];
    // The compress-first context, when something was actually refused. The split is
    // composed here from the `CostLine`, not carried as text, so the sentence cannot
    // quote a shape the price does not have.
    if let Some(hint) = &view.inventory.hint {
        lines.push(Line::from(format!(" {} wants", hint.purchase)));
        lines.push(Line::from(format!(
            " {} Compressed + {}.",
            grouped(hint.needed.compressed),
            grouped(hint.needed.raw)
        )));
        lines.push(Line::from(" You hold the value, not"));
        lines.push(Line::from(" the denomination."));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(" Free and lossless both ways."));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// `↑↓` walk the fifteen rows; `c` / `C` open the compression dialog on the row
/// under the cursor, in one direction or the other.
///
/// `←/→` are deliberately **not** claimed. They belong to the spinner inside that
/// dialog, which is a modal and is resolved before any screen is asked — a screen
/// that claimed them here would still be shadowed, so claiming them would only be a
/// second, dead answer to the same key.
pub fn map_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Up => Some(Action::CursorUp),
        KeyCode::Down => Some(Action::CursorDown),
        KeyCode::Char('c') => Some(Action::Compress),
        KeyCode::Char('C') => Some(Action::Decompress),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal, backend::TestBackend, buffer::Buffer, crossterm::event::KeyModifiers,
        style::Color,
    };

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
        assert!(row_with(&frame, "Material").contains("Raw"), "{frame}");
        assert!(
            row_with(&frame, "Material").contains("Compressed"),
            "{frame}"
        );
        // First and last of the closed fifteen, with a grouped count between them.
        assert!(row_with(&frame, "Stone").contains("4 508"), "{frame}");
        assert!(frame.contains("Amethyst"), "{frame}");
    }

    /// The table is fifteen fixed rows, not a listing of what is held — which is the
    /// difference between walking [`Material::ALL`] and walking the inventory.
    ///
    /// An [`Inventory`](skylode_core::inventory::Inventory) is *sparse*: an item at
    /// zero is not stored at all. So a run that has mined nothing must still draw all
    /// fifteen names with `0` beside them, and a row must not vanish when the player
    /// spends the last of something. Asserted from a real run, since that is the only
    /// place the sparse map can actually bite.
    #[test]
    fn a_run_that_holds_nothing_still_lists_all_fifteen_materials() {
        use std::time::Instant;

        use skylode_core::{game::GameState, mine_kind::MineKind};

        use crate::{cursor::Cursors, flash::Flashes, toast::Toasts};

        let state = GameState::new(0x5B1_0DE, std::time::UNIX_EPOCH);
        let view = View::from_state(
            &state,
            Cursors::new(MineKind::Stone, 0, 1),
            None,
            &Toasts::new(),
            &Flashes::new(),
            Instant::now(),
        );

        assert_eq!(view.inventory.rows.len(), 15);
        for row in &view.inventory.rows {
            assert_eq!(
                (row.raw, row.compressed),
                (0, 0),
                "{} is held in a run that has mined nothing",
                row.material.name()
            );
        }

        // And they are drawn, not merely projected: the first and last of the closed
        // fifteen, either side of the world groupings.
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
        assert!(frame.contains("Stone"), "{frame}");
        assert!(frame.contains("Crying Obsidian"), "{frame}");
        assert!(frame.contains("Amethyst"), "{frame}");
    }

    /// The panel's refusal half is absent when nothing has been refused, which is the
    /// state every run opens in.
    ///
    /// The rest of the panel is still drawn, and both halves are asserted: a
    /// `None` that blanked the whole panel would pass a test that only checked the
    /// sentence was gone.
    #[test]
    fn a_run_with_nothing_refused_draws_no_compress_first_sentence() {
        let mut view = View::sample();
        view.inventory.hint = None;

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
            !frame.contains("You hold the value"),
            "the panel invented a refusal that never happened: {frame}"
        );
        assert!(
            frame.contains("Compressible now"),
            "the missing hint took the rest of the panel with it: {frame}"
        );
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
        // `Held` labels the first of the two denomination lines, and the swap moved
        // which one that is: the label anchors Raw now, and Compressed is the bare
        // continuation line. Asserting the label against the wrong line is what the
        // column swap left behind.
        assert!(frame.contains("Held     480 Raw"), "{frame}");
        assert!(frame.contains("2 Compressed"), "{frame}");
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
    fn the_table_walks_on_the_arrows() {
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            Some(Action::CursorUp)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            Some(Action::CursorDown)
        );
        // `←/→` are *not* claimed here. They belong to the compression spinner, which
        // is a modal and is resolved before any screen is asked — a screen that
        // claimed them would shadow the dialog it opened.
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            None
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            None
        );
    }

    /// The two conversions differ by the shift key alone, which is what makes them
    /// read as one binding with a direction rather than two unrelated letters.
    #[test]
    fn the_case_of_the_key_chooses_the_direction() {
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)),
            Some(Action::Compress)
        );
        assert_eq!(
            map_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT)),
            Some(Action::Decompress)
        );
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
    fn a_cursor_on_no_row_at_all_empties_the_panel_instead_of_panicking() {
        // The `find` guard in `compress`. A typed cursor has already made the old
        // failure — an index past the end — *unrepresentable*, which is most of what
        // typing it was for; what is left is a table that does not list the selected
        // material at all, and the guard is what keeps that a blank panel rather than
        // a panic. A panic in a draw call takes the terminal down mid-frame.
        //
        // The screen still renders: the box, its title and the table beside it are
        // all there, and only the panel's contents are missing. That is the whole
        // claim, so both halves are asserted — a guard that blanked the screen would
        // pass a test that only checked for the absence of a panic.
        let mut view = View::sample();
        view.inventory.rows.clear();

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
