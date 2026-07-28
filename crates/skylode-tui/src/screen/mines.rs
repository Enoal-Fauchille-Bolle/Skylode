//! The Mines screen — pick a world and a mine, slide the richness dial (UI.md §5.2).
//!
//! Master-detail: a list of the twelve mines under three world headers on the left,
//! the selected mine's gate, size and richness on the right. Fifteen rows — twelve
//! mines plus three headers — fit in twenty, so this is the one list screen that
//! never needs a scrollbar at 80×24.
//!
//! The **richness dial** is drawn only on the three two-material mines (Quartz,
//! Obsidian, End). On the nine same-material mines enriching is pure gain — the
//! dial has one sensible position — so the block is replaced by a flat readout.
//! Whether a mine is two-material is read from its two materials, not carried: a
//! mine whose common and value materials differ is one the dial has a real choice
//! on.

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use skylode_core::world::World;

use crate::{action::Action, format::justified, screen::panel, theme, view::View};

/// The list panel's share of the row, against [`DETAIL_WEIGHT`] — and still the 38
/// columns UI-EN.md §5.3 counted, per the module note on `screen`.
const LIST_WEIGHT: u16 = 38;

/// The detail pane's share — the other 42 of the counted 80.
const DETAIL_WEIGHT: u16 = 42;

/// How many cells the dial bar spans between its `◄` and `►` arrows.
const DIAL_WIDTH: usize = 20;

/// Columns kept clear on the right of the list, so a size or a `✓` does not abut
/// the border the way the frame keeps them apart.
const LIST_MARGIN: usize = 2;

/// Draws the list, the detail pane, and the footer.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    let [body, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    let [list_area, detail_area] = Layout::horizontal([
        Constraint::Fill(LIST_WEIGHT),
        Constraint::Fill(DETAIL_WEIGHT),
    ])
    .areas(body);
    list(frame, list_area, view);
    detail(frame, detail_area, view);

    let footer = " ↑↓  select     Enter  mine it     ← →  richness dial     Tab  next screen";
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme::MUTED)),
        footer_area,
    );
}

/// The mine list, grouped under three world headers, the cursor marked `▸`.
fn list(frame: &mut Frame, area: Rect, view: &View) {
    let block = panel(" Mines ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = (inner.width as usize).saturating_sub(LIST_MARGIN);
    let mut lines = Vec::new();
    for world in [World::Overworld, World::Nether, World::End] {
        // The header carries the world's own gate — a level, ticked against the
        // player's — while the Overworld, open from level 1, shows only a `✓`.
        let status = if world.unlock_level() <= 1 {
            "✓".to_owned()
        } else {
            let tick = if world.is_unlocked_at(view.player_level) {
                "✓"
            } else {
                "✗"
            };
            format!("Lv {}  {tick}", world.unlock_level())
        };
        // `marked` after `justified`, never before: the padding is computed across
        // the whole row, so styling has to be the last thing that happens to it.
        lines.push(theme::marked(&justified(
            &format!(" {}", world.name()),
            &status,
            width,
        )));

        for row in view.mines.rows.iter().filter(|r| r.kind.world() == world) {
            let mark = if row.kind == view.mines.selected {
                " ▸ "
            } else {
                "   "
            };
            let left = format!("{mark}{}", row.kind.name());
            lines.push(theme::marked(&justified(&left, &row.detail, width)));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The detail pane: the selected mine's materials, gates, size, and the dial.
fn detail(frame: &mut Frame, area: Rect, view: &View) {
    let selected = view.mines.selected;
    let detail = &view.mines.detail;

    let block = panel(&format!(" {} Mine ", selected.name()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (width, height) = (detail.size.0, detail.size.1);
    let mut lines = vec![
        // Materials are derived: the common on the left, the value on the right,
        // which on a same-material mine reads the block's own two names.
        Line::from(format!(
            " {}  +  {}",
            selected.common_material().name(),
            selected.value_material().name(),
        )),
        Line::from(""),
        // Both carry a `✓`/`✗` of their own, so both go through `marked`.
        theme::marked(&format!(" World      {}", detail.world_line)),
        theme::marked(&format!(" Gate       {}", detail.gate_line)),
        Line::from(format!(
            " Size       {width} x {height} = {}    level {}",
            width * height,
            detail.size_level,
        )),
        Line::from(format!(
            " Blocks     {} / {}",
            detail.blocks_standing, detail.blocks_total,
        )),
        Line::from(format!(
            " Richness   level {} / {}",
            detail.richness_level, detail.richness_max,
        )),
        Line::from(""),
    ];

    // The dial exists only where the two materials differ; elsewhere enriching is
    // pure gain and the block is a flat readout rather than a slider.
    if selected.common_material() != selected.value_material() {
        let filled = (detail.value_percent as usize * DIAL_WIDTH) / 100;
        // Spans rather than `marked` here: `█` and `░` are not marks, and this row
        // is built by `format!` alone — no `justified` padding to preserve — so the
        // two halves can be split safely. Same accent/muted pair as the gauges and
        // the scrollbar, because the dial is one more "how far along" bar.
        lines.push(Line::from(vec![
            Span::raw(" Dial   ◄ "),
            Span::styled("█".repeat(filled), Style::default().fg(theme::ACCENT)),
            Span::styled(
                "░".repeat(DIAL_WIDTH.saturating_sub(filled)),
                Style::default().fg(theme::MUTED),
            ),
            Span::raw(" ►"),
        ]));
        lines.push(Line::from(format!("        {}", detail.dial_split)));
        lines.push(Line::from(""));
        lines.push(Line::from("        free, reversible, any time"));
        lines.push(Line::from(""));
        for note in &detail.note {
            lines.push(Line::from(format!(" {note}")));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(" ← →  move the dial"));
    } else {
        lines.push(Line::from(" Richness is pure gain here — enrich freely."));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

/// `↑↓` walk the list, `←→` move the richness dial, `Enter` enters the mine.
///
/// Every arm is a plain translation, because this function has no state to consult:
/// *which* mine `Enter` selects is decided in [`crate::app::App::update`], which is
/// where the cursor lives. Anything else is `None` — "not mine" — so
/// [`crate::keymap`] can fall through rather than swallowing the key.
pub fn map_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Up => Some(Action::CursorUp),
        KeyCode::Down => Some(Action::CursorDown),
        KeyCode::Left => Some(Action::AdjustLeft),
        KeyCode::Right => Some(Action::AdjustRight),
        KeyCode::Enter => Some(Action::Confirm),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    use super::*;

    /// Renders `view` through the Mines screen into an 80×24 buffer.
    fn render_view(view: &View) -> Buffer {
        let mut terminal = match Terminal::new(TestBackend::new(80, 24)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        if let Err(infallible) = terminal.draw(|frame| {
            let area = frame.area();
            render(frame, area, view);
        }) {
            match infallible {}
        }
        terminal.backend().buffer().clone()
    }

    /// The sample view — Obsidian selected, a two-material mine.
    fn render_screen() -> Buffer {
        render_view(&View::sample())
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

    /// Just the list's columns (0..38), joined per row — the detail pane repeats
    /// several of the list's words (`Obsidian`, `Nether`), so a list-only slice is
    /// what lets a test name a list row without matching the pane beside it.
    fn list_panel(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| (0..38).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The one row that contains `needle`, or the whole text on failure.
    fn row_with<'a>(text: &'a str, needle: &str) -> &'a str {
        text.lines()
            .find(|line| line.contains(needle))
            .unwrap_or(text)
    }

    #[test]
    fn the_list_groups_mines_under_their_three_worlds() {
        let list = list_panel(&render_screen());
        // The three headers, ticked against the mining level, not a stored flag.
        assert!(row_with(&list, "Overworld").contains('✓'), "{list}");
        assert!(row_with(&list, "Nether").contains("Lv 15"), "{list}");
        let end_header = row_with(&list, "End ");
        assert!(
            end_header.contains("Lv 30") && end_header.contains('✗'),
            "{list}"
        );
        // A mine from each: its size and richness in the right column.
        assert!(row_with(&list, "Stone").contains("20 x 10"), "{list}");
        assert!(row_with(&list, "Stone").contains("R 9"), "{list}");
    }

    #[test]
    fn the_cursor_marks_the_selected_mine() {
        let list = list_panel(&render_screen());
        let obsidian = row_with(&list, "Obsidian");
        assert!(
            obsidian.contains('▸'),
            "selected mine lacks the cursor: {obsidian:?}"
        );
        assert!(!row_with(&list, "Stone").contains('▸'), "{list}");
    }

    #[test]
    fn the_locked_end_mine_shows_its_reason() {
        let list = list_panel(&render_screen());
        // The End mine is drawn locked with its gate named, not hidden.
        let end_mine = row_with(&list, "locked");
        assert!(end_mine.contains("Netherite"), "{end_mine:?}");
    }

    #[test]
    fn the_detail_pane_describes_the_selected_mine() {
        let frame = whole_frame(&render_screen());
        // Title, materials (derived), the two gates, size, blocks, richness.
        assert!(frame.contains("Obsidian Mine"), "{frame}");
        assert!(frame.contains("Obsidian  +  Crying Obsidian"), "{frame}");
        assert!(
            row_with(&frame, "Gate").contains("Diamond pickaxe"),
            "{frame}"
        );
        assert!(row_with(&frame, "Size").contains("8 x 5 = 40"), "{frame}");
        assert!(row_with(&frame, "Size").contains("level 3"), "{frame}");
        assert!(row_with(&frame, "Blocks").contains("31 / 40"), "{frame}");
        assert!(
            row_with(&frame, "Richness").contains("level 6 / 9"),
            "{frame}"
        );
    }

    #[test]
    fn the_dial_is_drawn_for_a_two_material_mine() {
        let frame = whole_frame(&render_screen());
        // Obsidian is two-material, so the slider is drawn between its arrows.
        let dial = row_with(&frame, "Dial");
        assert!(dial.contains('◄') && dial.contains('►'), "{dial:?}");
        assert!(dial.contains('█') && dial.contains('░'), "{dial:?}");
        assert!(frame.contains("Crying 64%   Obsidian 36%"), "{frame}");
        assert!(frame.contains("optimum, not a maximum."), "{frame}");
    }

    #[test]
    fn a_same_material_mine_shows_a_flat_readout_not_a_dial() {
        // Selecting Iron — whose common and value materials are the same — must
        // replace the dial with the flat readout: on a one-material mine enriching
        // is pure gain, so there is no slider to move.
        use skylode_core::mine_kind::MineKind;

        let mut view = View::sample();
        view.mines.selected = MineKind::Iron;
        let frame = whole_frame(&render_view(&view));
        assert!(
            !frame.contains("Dial"),
            "a one-material mine drew a dial: {frame}"
        );
        assert!(frame.contains("pure gain"), "{frame}");
    }

    #[test]
    fn the_footer_names_select_mine_and_the_dial() {
        let buffer = render_screen();
        let last = (0..buffer.area.width)
            .map(|x| buffer[(x, 23)].symbol())
            .collect::<String>();
        assert!(last.contains("↑↓  select"), "{last:?}");
        assert!(last.contains("Enter  mine it"), "{last:?}");
        assert!(last.contains("← →  richness dial"), "{last:?}");
    }

    /// The foreground of the first cell drawn with `glyph`, or `None` if no cell is.
    ///
    /// Looking the cell up **by its glyph** is what makes the assertions below test
    /// both channels in one line: a build that dropped the mark fails on the `None`,
    /// and a build that mis-coloured it fails on the colour. A helper that took a
    /// coordinate would have tested only the second.
    fn fg_of(buffer: &Buffer, glyph: &str) -> Option<Color> {
        buffer
            .content()
            .iter()
            .find(|cell| cell.symbol() == glyph)
            .map(|cell| cell.fg)
    }

    #[test]
    fn the_marks_keep_their_glyph_and_take_their_colour() {
        // UI.md §4.4: colour doubles a glyph, never replaces it. The sample has all
        // three — the Overworld's `✓`, the End's `✗`, and the cursor on Obsidian.
        let buffer = render_screen();
        assert_eq!(fg_of(&buffer, "✓"), Some(theme::AFFORDABLE));
        assert_eq!(fg_of(&buffer, "✗"), Some(theme::REFUSED));
        assert_eq!(fg_of(&buffer, "▸"), Some(theme::ACCENT));
    }

    #[test]
    fn the_dial_is_drawn_in_the_same_pair_as_every_other_progress_bar() {
        let buffer = render_screen();
        assert_eq!(fg_of(&buffer, "█"), Some(theme::ACCENT));
        assert_eq!(fg_of(&buffer, "░"), Some(theme::MUTED));
    }
}
