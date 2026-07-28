//! The Levels screen — the level roadmap (UI.md §5.6).
//!
//! A roadmap with **no detail pane**, unlike Upgrades and Mines: each level *is*
//! one line, so there is nothing to break out into a second panel. What it does
//! carry is two distinct marks — `●` for the level the player is on, `▸` for the
//! list cursor — which coincide (`▸●`) until the cursor scrolls away, plus a `✓`
//! on every level already reached. On this screen `✓` reads "already yours":
//! nothing is bought here (UI.md §6.11).
//!
//! The distance to the next level rides in the **Block title**, spending no row,
//! and a `Scrollbar` on the right edge stands in for the whole ladder that the rows
//! on screen are a slice of. `view.levels` carries all of it — the windowing is this
//! screen's own decision, taken against the rows the frame actually has — so the
//! bar's length is that list's length. Taking it from the list being drawn, rather
//! than from the core's `LEVEL_CAP`, is what stops the thumb describing a different
//! list from the rows beside it.

use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::Paragraph,
};

use crate::{
    action::Action,
    format::{grouped, justified},
    screen::{panel, scrollbar, window},
    theme,
    view::View,
};

/// Draws the roadmap and its footer.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    // The bordered roadmap fills the screen; the footer is the last row under it.
    let [roadmap_area, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    // The distance to the next level rides in the title, so no content row pays
    // for it: `Lv 23 · 1 240 / 2 300 XP to Lv 24`.
    let title = format!(
        " Levels — Lv {} · {} / {} XP to Lv {} ",
        view.player_level,
        grouped(view.xp),
        grouped(view.xp_to_next),
        view.player_level + 1,
    );
    let block = panel(&title);
    let inner = block.inner(roadmap_area);
    frame.render_widget(block, roadmap_area);

    // A header row over the list; below it, the rows themselves beside a one-column
    // scrollbar. Splitting the body — not the whole inner area — is what keeps the
    // scrollbar off the header, where the frame draws no thumb.
    let [header_area, body_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    let [rows_area, bar_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(body_area);

    // Two columns short of the row width, so the XP never abuts the scrollbar the
    // way the frame keeps them apart (`1 300  ░`).
    let width = (rows_area.width as usize).saturating_sub(2);
    frame.render_widget(
        Paragraph::new(justified("    Lv    Grants", "XP", width))
            .style(Style::default().fg(theme::MUTED)),
        header_area,
    );

    // The cursor sits on the current level until scrolling exists to move it
    // (phase 7), so the two marks coincide at `player_level` for now.
    let current = view.player_level;
    let selected = view.player_level;

    // The roadmap is the whole 1..=LEVEL_CAP ladder, so the window is computed here,
    // against the rows this frame actually has. `selected - 1` is the cursor's index:
    // levels are one-based and the list is not, and `saturating_sub` covers the
    // level-zero case the rules never produce but the type still permits.
    let cursor = (selected as usize).saturating_sub(1);
    let visible = usize::from(rows_area.height);
    let range = window(view.levels.len(), cursor, view.levels_offset, visible);

    let lines: Vec<Line> = view.levels[range.clone()]
        .iter()
        .map(|row| {
            let left = format!(
                "{}{:<3}   {}",
                mark(row.level, current, selected),
                row.level,
                row.grants,
            );
            // This is the row the whole mark palette was designed around: on the
            // current level the cursor and the position render adjacent as `▸●`,
            // so the two must not collapse into one colour.
            theme::marked(&justified(&left, &grouped(row.xp), width))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows_area);

    // The whole ladder's length, and the window's *own* start rather than the view's
    // stored offset: `window` may have moved it to keep the cursor on screen, and a
    // thumb pointing at where the list used to be would report a scroll that did not
    // happen.
    scrollbar(frame, bar_area, view.levels.len(), range.start);

    let footer =
        format!(" ↑↓  scroll     Home  jump to Lv {current}     Tab  next screen     ?  help");
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme::MUTED)),
        footer_area,
    );
}

/// The four-column mark field: which of `✓ ● ▸` (or a pair) a level shows.
///
/// `▸` is the cursor and `●` the current level; they combine to `▸●` when they
/// coincide. A level below the current one that is *not* the cursor reads `✓`,
/// "already yours". Every arm is padded to four columns so the level numbers
/// beside it line up whatever mark is drawn.
fn mark(level: u32, current: u32, selected: u32) -> &'static str {
    match (level == selected, level == current) {
        (true, true) => " ▸● ",
        (true, false) => "  ▸ ",
        (false, true) => "  ● ",
        _ if level < current => "  ✓ ",
        _ => "    ",
    }
}

/// No contextual bindings yet; `↑↓` scrolls, `Home` jumps to the current level.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    use super::*;

    /// Renders the Levels screen alone into an 80×24 buffer.
    fn render_screen() -> Buffer {
        render_sized(80, 24)
    }

    /// The same, at an arbitrary size — for the responsive assertions.
    fn render_sized(width: u16, height: u16) -> Buffer {
        let view = View::sample();
        let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
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

    /// How many roadmap rows the buffer draws — the rows inside the box that carry
    /// an `XP`-column figure, which every level row does and no chrome row does.
    fn levels_drawn(buffer: &Buffer) -> usize {
        whole_frame(buffer)
            .lines()
            .filter(|line| line.starts_with('│'))
            .filter(|line| line.contains("+") || line.contains("opens"))
            .count()
    }

    #[test]
    fn the_counted_window_still_opens_on_level_thirteen() {
        // The view now carries all fifty levels, so *which* of them are drawn is the
        // window's decision — and it has to still open on 13, where UI.md §5.6 does.
        //
        // Twenty rows, not the wireframe's nineteen: these tests hand the screen the
        // whole 24-row terminal, while the app gives it 23 and keeps one for the tab
        // bar. The extra row is real either way — the old fixture carried nineteen
        // rows into a box with room for twenty, and left the last one blank.
        let frame = whole_frame(&render_screen());
        assert_eq!(levels_drawn(&render_screen()), 20);
        assert!(frame.contains("The Nether opens"), "{frame}");
        assert!(frame.contains("+233 End Stone"), "{frame}");
        // Level 12 sits above the window, so its own grants line is absent. Asserted
        // on that line's text rather than on `row_with(" 12 ")`, whose fallback hands
        // back the whole frame when nothing matches — which is exactly the case a
        // negative assertion is testing for, so it would always pass.
        let below_the_window = &View::sample().levels[11].grants;
        assert!(!frame.contains(below_the_window.as_str()), "{frame}");
    }

    #[test]
    fn a_taller_terminal_shows_more_of_the_roadmap() {
        // The ladder is fifty levels; a 24-row terminal shows nineteen of them and a
        // 48-row one has to show more, because the extra levels are now in the view.
        let counted = levels_drawn(&render_screen());
        let tall = levels_drawn(&render_sized(80, 48));
        assert!(
            tall > counted,
            "a 48-row terminal drew {tall} levels, no more than the {counted} at 24"
        );
        assert!(tall <= View::sample().levels.len());
    }

    /// The one row that contains `needle`, or the whole frame on failure.
    fn row_with<'a>(frame: &'a str, needle: &str) -> &'a str {
        frame
            .lines()
            .find(|line| line.contains(needle))
            .unwrap_or(frame)
    }

    #[test]
    fn the_title_carries_the_distance_to_the_next_level() {
        let frame = whole_frame(&render_screen());
        let title = row_with(&frame, "Levels");
        assert!(title.contains("Lv 23"), "{title:?}");
        assert!(title.contains("1 240 / 2 300"), "{title:?}");
        assert!(title.contains("XP to Lv 24"), "{title:?}");
    }

    #[test]
    fn the_current_level_carries_both_marks_and_earlier_ones_are_ticked() {
        let frame = whole_frame(&render_screen());
        // Level 23 is where the player stands: cursor and current mark coincide.
        let current = row_with(&frame, "+115 Quartz");
        assert!(
            current.contains("▸●"),
            "current row lacks both marks: {current:?}"
        );
        assert!(current.contains("23"), "{current:?}");
        // A level already reached is ticked, a future one bears no mark.
        assert!(row_with(&frame, "The Nether opens").contains('✓'));
        let future = row_with(&frame, "+120 Quartz");
        assert!(!future.contains('✓') && !future.contains('●'), "{future:?}");
    }

    #[test]
    fn every_row_shows_its_grants_and_per_level_xp() {
        let frame = whole_frame(&render_screen());
        // World rows look different — a world, no loot — and still fit one line.
        assert!(frame.contains("The Nether opens"), "{frame}");
        assert!(frame.contains("The End opens"), "{frame}");
        // The XP column is per-level (level × 100), counted from zero.
        assert!(row_with(&frame, "+65 Lapis").contains("1 300"), "{frame}");
        assert!(row_with(&frame, "End Stone").contains("3 100"), "{frame}");
    }

    #[test]
    fn the_scrollbar_draws_a_thumb_over_a_track() {
        let frame = whole_frame(&render_screen());
        // On this screen `█` and `░` can only be the scrollbar — no gauges here —
        // so their presence is the scrollbar rendering rather than collapsing.
        assert!(frame.contains('█'), "no scrollbar thumb: {frame}");
        assert!(frame.contains('░'), "no scrollbar track: {frame}");
    }

    #[test]
    fn the_footer_names_scroll_and_the_home_jump() {
        let buffer = render_screen();
        let last = (0..buffer.area.width)
            .map(|x| buffer[(x, 23)].symbol())
            .collect::<String>();
        assert!(last.contains("↑↓  scroll"), "{last:?}");
        assert!(last.contains("Home  jump to Lv 23"), "{last:?}");
        assert!(last.contains("?  help"), "{last:?}");
    }

    #[test]
    fn the_mark_column_tells_the_three_states_and_their_overlap() {
        // The cursor overlaps the current level (▸●); a reached level below the
        // cursor is ticked; a future level is blank; and a cursor parked away from
        // the current level shows ▸ and ● apart, which scrolling will produce.
        assert_eq!(mark(23, 23, 23), " ▸● ");
        assert_eq!(mark(15, 23, 23), "  ✓ ");
        assert_eq!(mark(24, 23, 23), "    ");
        assert_eq!(mark(23, 23, 20), "  ● ");
        assert_eq!(mark(20, 23, 20), "  ▸ ");
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
    fn the_adjacent_cursor_and_position_marks_do_not_share_a_colour() {
        // This screen is the reason `CURRENT` is magenta rather than a second
        // green: `▸` and `●` render in touching cells on the current level, so a
        // shared colour would merge them into one four-column smudge.
        let buffer = render_screen();
        assert_eq!(fg_of(&buffer, "▸"), Some(theme::ACCENT));
        assert_eq!(fg_of(&buffer, "●"), Some(theme::CURRENT));
        assert_eq!(fg_of(&buffer, "✓"), Some(theme::AFFORDABLE));
        assert_ne!(theme::ACCENT, theme::CURRENT);
    }
}
