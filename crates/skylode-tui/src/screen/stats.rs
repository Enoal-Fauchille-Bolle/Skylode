//! The Stats screen — progression, prestige, run progress, history (UI.md §5.5).
//!
//! Three panels. **Progression** on the left is the numbers a run is measured by;
//! **This run** on the right top is run *progress*, not achievements — every row
//! is a predicate that resets with a prestige, which is why a panel that un-ticks
//! is honest here where a "Milestones" panel would be broken. **History** below it
//! is the toast log verbatim — one buffer, two renderings, the toast being its
//! three-second tail.
//!
//! The worlds table and the level cap in the Progression panel are **derived**,
//! not carried: `World::is_unlocked_at` and `LEVEL_CAP` already answer them from
//! the mining level the save holds, so a second copy would be an invariant to keep.

use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::Paragraph,
};
use skylode_core::{tunables::LEVEL_CAP, world::World};

use crate::{
    action::Action,
    format::{grouped, justified},
    screen::panel,
    theme,
    view::View,
};

/// Trailing columns kept clear on the right of a flush-right value, so numbers do
/// not touch the border the way the frame keeps a margin (`418 297    │`).
const RIGHT_MARGIN: usize = 3;

/// Draws the three panels and the footer.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    let [body, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    // Progression fills the left; the right column stacks This run over History.
    let [left, right] =
        Layout::horizontal([Constraint::Length(30), Constraint::Min(0)]).areas(body);
    progression(frame, left, view);

    let [this_run_area, history_area] =
        Layout::vertical([Constraint::Length(10), Constraint::Min(0)]).areas(right);
    this_run(frame, this_run_area, view);
    history(frame, history_area, view);

    let footer = " ↑↓  scroll history     p  prestige     Tab  next screen     ?  help";
    frame.render_widget(
        Paragraph::new(footer).style(Style::default().fg(theme::MUTED)),
        footer_area,
    );
}

/// The Progression panel: level, XP, the worlds, prestige, and lifetime counters.
fn progression(frame: &mut Frame, area: Rect, view: &View) {
    let block = panel(" Progression ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let s = &view.stats;

    let mut lines = vec![
        stat(
            "Mining level",
            &format!("{} / {LEVEL_CAP}", view.player_level),
            width,
        ),
        stat(
            "XP",
            &format!("{} / {}", grouped(view.xp), grouped(view.xp_to_next)),
            width,
        ),
        Line::from(""),
    ];

    // The two gated worlds, ticked against the mining level rather than a stored
    // flag: `Lv 15 ✓`, `Lv 30 ✗`.
    for world in [World::Nether, World::End] {
        let ok = if world.is_unlocked_at(view.player_level) {
            "✓"
        } else {
            "✗"
        };
        let left = format!(" {:<11}Lv {}", world.name(), world.unlock_level());
        lines.push(theme::marked(&justified(
            &left,
            ok,
            width.saturating_sub(RIGHT_MARGIN),
        )));
    }

    lines.push(Line::from(""));
    lines.push(inline("Prestige", &format!("rank {}", s.prestige_rank)));
    lines.push(inline("Multiplier", &s.multiplier));
    lines.push(inline("Next rank", &s.next_multiplier));
    lines.push(stat(
        "Cost",
        &format!("{} {}", grouped(s.prestige_cost), s.prestige_material),
        width,
    ));
    lines.push(stat(
        "Held",
        &format!("{} {}", grouped(s.prestige_held), s.prestige_material),
        width,
    ));
    lines.push(Line::from(""));
    lines.push(stat("Blocks broken", &grouped(s.blocks_broken), width));
    lines.push(stat("Playtime", &s.playtime, width));
    lines.push(stat("This run", &s.this_run, width));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// The This run panel: run-progress rows, each marked and optionally detailed.
fn this_run(frame: &mut Frame, area: Rect, view: &View) {
    let block = panel(" This run ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let width = inner.width as usize;
    let lines: Vec<Line> = view
        .stats
        .milestones
        .iter()
        .map(|milestone| {
            let mark = if milestone.done {
                "✓"
            } else if milestone.current {
                "▸"
            } else {
                " "
            };
            let left = format!(" {mark}  {}", milestone.text);
            theme::marked(&justified(
                &left,
                &milestone.detail,
                width.saturating_sub(RIGHT_MARGIN),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The History panel: the event log, one line each, the toast log verbatim.
fn history(frame: &mut Frame, area: Rect, view: &View) {
    let block = panel(" History ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = view
        .stats
        .history
        .iter()
        .map(|entry| Line::from(format!(" {entry}")))
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

/// A `label … value` row with the value flush right and a margin off the border.
fn stat(label: &str, value: &str, width: usize) -> Line<'static> {
    Line::from(justified(
        &format!(" {label}"),
        value,
        width.saturating_sub(RIGHT_MARGIN),
    ))
}

/// A `label   value` row with the value at a fixed column — the shape the prestige
/// rows take, which the frame left-aligns rather than pushing to the right edge.
fn inline(label: &str, value: &str) -> Line<'static> {
    Line::from(format!(" {label:<11}{value}"))
}

/// No contextual bindings yet; `↑↓` scrolls the history, `p` opens prestige.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    use super::*;

    /// Renders the Stats screen alone into an 80×24 buffer.
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

    /// Just the Progression panel's columns (0..30), joined per row.
    ///
    /// The panels sit side by side, so a whole-frame row holds a cell from each —
    /// and `Nether`/`End` appear in *both* the Progression rows and the This-run
    /// goals. Slicing the left panel off is what lets a test name the world row it
    /// means without matching `Reach the Nether` on the same terminal line.
    fn left_panel(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| (0..30).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_three_panels_are_all_titled() {
        let frame = whole_frame(&render_screen());
        assert!(frame.contains("Progression"), "{frame}");
        assert!(frame.contains("This run"), "{frame}");
        assert!(frame.contains("History"), "{frame}");
    }

    #[test]
    fn progression_shows_level_xp_and_prestige_figures() {
        let frame = whole_frame(&render_screen());
        assert!(
            row_with(&frame, "Mining level").contains("23 / 50"),
            "{frame}"
        );
        assert!(row_with(&frame, "XP").contains("1 240 / 2 300"), "{frame}");
        assert!(row_with(&frame, "Prestige").contains("rank II"), "{frame}");
        assert!(row_with(&frame, "Multiplier").contains("×1.20"), "{frame}");
        assert!(
            row_with(&frame, "Cost").contains("6 540 Amethyst"),
            "{frame}"
        );
        assert!(
            row_with(&frame, "Blocks broken").contains("418 297"),
            "{frame}"
        );
    }

    #[test]
    fn the_worlds_are_ticked_against_the_mining_level_not_a_stored_flag() {
        let prog = left_panel(&render_screen());
        // At level 23: the Nether (Lv 15) is open, the End (Lv 30) is not.
        let nether = row_with(&prog, "Nether");
        assert!(
            nether.contains("Lv 15") && nether.contains('✓'),
            "{nether:?}"
        );
        let end = row_with(&prog, "End");
        assert!(end.contains("Lv 30") && end.contains('✗'), "{end:?}");
    }

    #[test]
    fn this_run_marks_done_current_and_pending_goals() {
        let frame = whole_frame(&render_screen());
        // Done goals tick, the goal in progress points, and it carries its
        // distance; a not-yet goal shows neither mark.
        assert!(row_with(&frame, "Break your first block").contains('✓'));
        let end = row_with(&frame, "Reach the End");
        assert!(end.contains('▸') && end.contains("23/30"), "{end:?}");
        let netherite = row_with(&frame, "Netherite pickaxe");
        assert!(!netherite.contains('▸'), "{netherite:?}");
    }

    #[test]
    fn history_is_the_event_log_verbatim() {
        let frame = whole_frame(&render_screen());
        assert!(
            frame.contains("20:14  Excavator!  +1 Compressed Iron"),
            "{frame}"
        );
        assert!(frame.contains("Welcome back — 6h away"), "{frame}");
    }

    #[test]
    fn the_footer_names_the_history_scroll_and_prestige() {
        let buffer = render_screen();
        let last = (0..buffer.area.width)
            .map(|x| buffer[(x, 23)].symbol())
            .collect::<String>();
        assert!(last.contains("↑↓  scroll history"), "{last:?}");
        assert!(last.contains("p  prestige"), "{last:?}");
        assert!(last.contains("?  help"), "{last:?}");
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
    fn the_marks_keep_their_glyph_and_take_their_colour() {
        // `✓` reads "already yours" here rather than "you can buy it", and takes
        // the same colour anyway: both readings agree that the row is settled in
        // the player's favour, which is all the colour claims.
        let buffer = render_screen();
        assert_eq!(fg_of(&buffer, "✓"), Some(theme::AFFORDABLE));
        assert_eq!(fg_of(&buffer, "✗"), Some(theme::REFUSED));
        assert_eq!(fg_of(&buffer, "▸"), Some(theme::ACCENT));
    }
}
