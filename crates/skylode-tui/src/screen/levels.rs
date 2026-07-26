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
//! and a `Scrollbar` on the right edge stands in for the 1..50 ladder the visible
//! window is a slice of — its length comes from the core's `LEVEL_CAP`, not from
//! how many rows the fixture happens to hold.

use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use skylode_core::tunables::LEVEL_CAP;

use crate::{
    action::Action,
    format::{grouped, justified},
    screen::panel,
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
        Paragraph::new(justified("    Lv    Grants", "XP", width)),
        header_area,
    );

    // The cursor sits on the current level until scrolling exists to move it
    // (phase 7), so the two marks coincide at `player_level` for now.
    let current = view.player_level;
    let selected = view.player_level;
    let lines: Vec<Line> = view
        .levels
        .iter()
        .map(|row| {
            let left = format!(
                "{}{:<3}   {}",
                mark(row.level, current, selected),
                row.level,
                row.grants,
            );
            Line::from(justified(&left, &grouped(row.xp), width))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows_area);

    scrollbar(frame, bar_area, view);

    let footer =
        format!(" ↑↓  scroll     Home  jump to Lv {current}     Tab  next screen     ?  help");
    frame.render_widget(Paragraph::new(footer), footer_area);
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

/// Draws the roadmap scrollbar, its thumb sized against the whole 1..50 ladder.
fn scrollbar(frame: &mut Frame, area: Rect, view: &View) {
    // Position is the first visible level's index into the full ladder, so the
    // thumb sits where the window is — not where the fixture's own first row is.
    let first_visible = view.levels.first().map_or(1, |row| row.level);
    let position = first_visible.saturating_sub(1) as usize;

    let mut state = ScrollbarState::new(LEVEL_CAP as usize).position(position);
    let bar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .begin_symbol(None)
        .end_symbol(None)
        .track_symbol(Some("░"))
        .thumb_symbol("█");
    frame.render_stateful_widget(bar, area, &mut state);
}

/// No contextual bindings yet; `↑↓` scrolls, `Home` jumps to the current level.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use super::*;

    /// Renders the Levels screen alone into an 80×24 buffer.
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
}
