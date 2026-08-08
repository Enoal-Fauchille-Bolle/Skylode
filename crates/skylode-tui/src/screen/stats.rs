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
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::Paragraph,
};
use skylode_core::{tunables::LEVEL_CAP, world::World};

use crate::{
    action::Action,
    format::{
        age, denominations, grouped_u64, holding, justified, multiplier, prestige_rank, truncate,
        xp_progress,
    },
    screen::{panel, scrollbar, window},
    theme,
    view::View,
};

/// Trailing columns kept clear on the right of a flush-right value, so numbers do
/// not touch the border the way the frame keeps a margin (`418 297    │`).
const RIGHT_MARGIN: usize = 3;

/// The Progression panel's share of the row, against [`RIGHT_COLUMN_WEIGHT`] — the
/// counted widths doubling as `Fill` weights, per the module note on `screen`.
const PROGRESSION_WEIGHT: u16 = 30;

/// The right column's share — the other 50 of the counted 80.
const RIGHT_COLUMN_WEIGHT: u16 = 50;

/// The margin the two price rows keep instead of [`RIGHT_MARGIN`].
///
/// **Two columns tighter, and the panel's width is the whole argument.** `Held` can
/// read `0 Compressed + 20 000` — twenty-one columns against a label and twenty-eight
/// of panel — so at the ordinary margin the value collides with its own label. These
/// two rows therefore sit one column off the border where the counters sit three. The
/// alternative was quoting the purse as a single total, which is the lie
/// [`holding`] exists to refuse.
const PRICE_MARGIN: usize = 1;

/// The column [`history`] keeps clear on the right, so a truncated line's `…` does not
/// sit against the scrollbar.
///
/// One and not [`RIGHT_MARGIN`]'s three: these lines are prose and the box is already
/// too narrow for what the game says into it, so every column spent on air is a word
/// the player does not get. It matches [`PRICE_MARGIN`] for the same reason — both are
/// rows where the content, not the alignment, is what the margin is competing with.
///
/// **It is measured against the rows, not the panel**, now that a column of the panel
/// belongs to the bar: the scrollbar is not a margin that happens to be drawn on, so
/// counting it as one would leave the `…` flush against the track. The Levels roadmap
/// keeps two here for the same shape and a different reason — its right-hand column is
/// a number, and a figure abutting a bar reads as a wider figure.
const HISTORY_MARGIN: usize = 1;

/// Draws the three panels and the footer.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    let [body, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    // Progression fills the left; the right column stacks This run over History.
    let [left, right] = Layout::horizontal([
        Constraint::Fill(PROGRESSION_WEIGHT),
        Constraint::Fill(RIGHT_COLUMN_WEIGHT),
    ])
    .areas(body);
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
        stat("XP", &xp_progress(view.xp, view.xp_to_next), width),
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

    // The prestige half comes from `view.prestige`, which the §6.8 preview reads too:
    // the box is opened from this panel with `p`, and two readings of the same trade
    // could quote two prices at the player in the space of one keystroke.
    let p = &view.prestige;
    lines.push(Line::from(""));
    lines.push(inline(
        "Prestige",
        &format!("rank {}", prestige_rank(p.rank)),
    ));
    lines.push(inline("Multiplier", &multiplier(p.multiplier_permille)));
    lines.push(inline("Next rank", &multiplier(p.next_multiplier_permille)));
    // **Two denominations, against the frame's flat total**, and the frame is what is
    // wrong: a prestige is paid as a `Cost`, so 6 540 raw Amethyst does not settle a
    // price of `65 Compressed + 40`. Quoting the total alone would let the preview
    // print a `✗` beside a `Held` that matches the `Cost` and say nothing about why.
    //
    // **The material moves to a line of its own**, which the frame does not draw, and
    // the panel's width is what settles it: 28 columns cannot hold `Cost  65 Compressed
    // + 40 Amethyst`, and of the two words that could go, the denominations are the
    // ones that decide whether the till accepts. Naming it once above the pair costs a
    // row the panel has and reads as the unit both figures are counted in.
    // `docs/UI.md` §5.5.1.
    lines.push(inline("Price in", p.material.name()));
    lines.push(price_row("Cost", &denominations(p.cost), width));
    // **The purse, not a re-split of its value.** `denominations` answers *"what
    // would a price of this size be owed in"*, which is a different question and the
    // wrong one here: a player holding 20 000 raw would read `200 Compressed`, own
    // none, and be refused with no way to see why.
    lines.push(price_row(
        "Held",
        &holding(p.held_compressed, p.held_raw),
        width,
    ));
    lines.push(Line::from(""));
    lines.push(stat("Blocks broken", &grouped_u64(s.blocks_broken), width));
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

/// The History panel: every announcement this session made, newest first.
///
/// **Four departures from the §5.5 frame, all found by rendering it** (`docs/UI.md`
/// §5.5.2). The stamp is an age and not a clock; the lines are truncated because the
/// real sentences do not fit; the selected row is drawn, which the frame does not
/// show and which the scroll needs — without it the first presses of `↑↓` move a
/// cursor inside the window and nothing on the screen changes; and a scrollbar is
/// drawn, which the frame has no column for.
fn history(frame: &mut Frame, area: Rect, view: &View) {
    let block = panel(" History ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The rows beside a one-column bar, on the Levels roadmap's own split. There is no
    // header to keep the bar off here, so the whole inner area divides — `Min(0)` first
    // so the text takes every column the terminal has beyond the one the bar is owed,
    // rather than freezing at the width §5.5 was counted at.
    let [rows_area, bar_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(inner);

    // **No stored offset, exactly as the Levels roadmap has none.** `window` derives
    // the visible slice from the cursor and a zero offset, which keeps the scroll
    // minimal — the box only moves when the selection would otherwise leave it — and
    // leaves one scroll position rather than two that could disagree.
    let history = &view.stats.history;
    let selected = view.stats.selected;
    let visible = usize::from(rows_area.height);
    let range = window(history.len(), selected, 0, visible);
    // The window's own start, so the drawn rows can be numbered back into the log and
    // the selected one found. Recomputed from the range rather than carried, because
    // `window` is what settled it and a second copy would be the disagreement the
    // missing offset exists to prevent. The bar is handed this same figure, for the
    // same reason it is the only one there is.
    let top = range.start;

    let width = (rows_area.width as usize).saturating_sub(HISTORY_MARGIN);
    let lines: Vec<Line> = history[range]
        .iter()
        .enumerate()
        .map(|(row, entry)| {
            // The age column is padded to a fixed width rather than flush-right,
            // because the sentence beside it is what the eye scans down: a ragged
            // left edge on the text would cost more than the ragged one on `1m`/`23h`.
            let line = format!(" {:<5}{}", age(entry.age_secs), entry.text);
            let line = Line::from(truncate(&line, width));
            if top + row == selected {
                // **Colour and no glyph.** Every other list in the game marks its
                // cursor with `▸`, and here that would spend a column of a box already
                // too narrow for what it prints — and put a mark in front of sentences
                // that read as a log rather than as rows to act on. Nothing is bought
                // from this list, so the cursor only has to say *where you are*.
                line.style(Style::default().fg(theme::ACCENT))
            } else {
                line
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), rows_area);

    // **`top` and never a stored offset**, which on this screen is not even a choice:
    // there is no second scroll position to get wrong, because `window` is what settles
    // where the box sits and it may have moved it to keep the cursor visible. A thumb
    // drawn from anything else would report a scroll that did not happen.
    //
    // Worth the column here where §5.5's frame spends none: the log is capped at 500
    // entries and *every* announcement enters it, including the ones too quiet to draw a
    // toast — so this is the one panel on the screen where "how much of it am I looking
    // at" is a real question. `scrollbar` draws nothing at all when the log fits, so a
    // fresh session pays for it in a blank column rather than in a stuck-looking thumb.
    scrollbar(frame, bar_area, history.len(), visible, top);
}

/// A `label … value` row with the value flush right and a margin off the border.
fn stat(label: &str, value: &str, width: usize) -> Line<'static> {
    Line::from(justified(
        &format!(" {label}"),
        value,
        width.saturating_sub(RIGHT_MARGIN),
    ))
}

/// A [`stat`] row that keeps [`PRICE_MARGIN`] instead of [`RIGHT_MARGIN`] — the two
/// rows whose value is a two-denomination figure and does not fit at the wider one.
fn price_row(label: &str, value: &str, width: usize) -> Line<'static> {
    Line::from(justified(
        &format!(" {label}"),
        value,
        width.saturating_sub(PRICE_MARGIN),
    ))
}

/// A `label   value` row with the value at a fixed column — the shape the prestige
/// rows take, which the frame left-aligns rather than pushing to the right edge.
fn inline(label: &str, value: &str) -> Line<'static> {
    Line::from(format!(" {label:<11}{value}"))
}

/// `↑↓` scroll the history; `p` opens the prestige preview.
///
/// **Bound here and not globally**, though prestige is reachable from nowhere else:
/// `p` is an ordinary letter, and a global claim would swallow it on the five screens
/// that do not own it — the same argument that keeps the sub-tab binding gated on the
/// Upgrades screen.
///
/// **The arrows are what this function was missing, and the footer had been promising
/// them for a phase.** Every other link in the chain existed — the cursor
/// ([`Cursors::history`](crate::cursor::Cursors::history)), the reducer's arm in
/// [`App::step_list_cursor`](crate::app::App), the projection, the drawn accent — so a
/// `↑` decoded to nothing here, fell out of [`crate::keymap`]'s rule 6, and the screen
/// simply did not move. Both halves were tested and the seam between them was not,
/// which is why `screen::tests::only_the_screens_with_bindings_claim_a_key` now names
/// this screen's row rather than leaving it to a catch-all.
///
/// They decode to the *list* gestures and not to a scroll of their own: §9 makes the
/// history a row cursor, so `↑↓` here mean exactly what they mean on the Levels roadmap
/// and the reducer picks the list from the open screen.
pub fn map_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Up => Some(Action::CursorUp),
        KeyCode::Down => Some(Action::CursorDown),
        KeyCode::Char('p') => Some(Action::OpenPrestige),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    use super::*;

    /// Renders the Stats screen alone into an 80×24 buffer, at the sample run.
    fn render_screen() -> Buffer {
        render_view(&View::sample())
    }

    /// The same, at a view the caller has adjusted — for the scroll, which is the one
    /// thing on this screen that a fixture cannot show both ends of.
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

    /// The column the History's scrollbar is drawn in, at 80 columns.
    ///
    /// The right column starts at 30, the panel's border takes 30 and 79, and the bar
    /// claims the last inner one. Spelled once because two tests need it and they must
    /// not disagree about which side of it they are looking at.
    const BAR_COLUMN: u16 = 78;

    /// One past the last column of History *text* — everything left of the bar.
    const TEXT_END: u16 = BAR_COLUMN;

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
        // The price in the shape it is paid in, and the material named once above the
        // pair rather than on both lines — `docs/UI.md` §5.5.1, and the panel's own 28
        // columns are the argument.
        assert!(row_with(&frame, "Price in").contains("Amethyst"), "{frame}");
        assert!(
            row_with(&frame, "Cost").contains("65 Compressed + 40"),
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

    /// The history prints the announcement in [`announce`](crate::announce)'s own
    /// words, newest first, behind a column saying how long ago it was said.
    ///
    /// **The stamp is an age and not a clock**, against the frame's `20:14`: the buffer
    /// holds [`Instant`](std::time::Instant)s, which are monotonic and know nothing
    /// about a time of day, and `std` cannot render a *local* one without being told
    /// the zone. An age needs neither, and "two minutes ago" is the better answer in a
    /// log anyway. `docs/UI.md` §5.5.2.
    #[test]
    fn history_prints_the_announcement_behind_its_age() {
        let frame = whole_frame(&render_screen());
        assert!(
            frame.contains("1m   Excavator!  +1 Compressed Iron"),
            "{frame}"
        );
        // The oldest entries in the fixture are hours and a day back, so the column
        // shows more than one unit.
        assert!(frame.contains("Explosive — 9 blocks"), "{frame}");
    }

    /// The window is derived from the cursor and there is no second scroll position:
    /// moving the selection past the bottom of the box slides the box, and the newest
    /// entry leaves the top.
    #[test]
    fn scrolling_the_history_slides_the_window_off_its_newest_entry() {
        let mut view = View::sample();
        let top = whole_frame(&render_view(&view));
        assert!(top.contains("Excavator!  +1 Compressed Iron"), "{top}");

        // Past the eleven rows the box has at 80×24.
        view.stats.selected = view.stats.history.len() - 1;
        let bottom = whole_frame(&render_view(&view));
        assert!(
            !bottom.contains("Excavator!  +1 Compressed Iron"),
            "the window never moved: {bottom}"
        );
        assert!(bottom.contains("Mine refilled"), "{bottom}");
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

    /// A sentence too long for the box is cut and marked, not clipped silently.
    ///
    /// **The frame under-counts what the game says.** §5.5 draws `+80 A. Debris`, an
    /// abbreviation nothing produces; the real line is fifty-five columns against
    /// forty-six of box. Ratatui would clip it flush against the border, where a cut
    /// word and a word that merely ends there look identical — the `…` is what
    /// separates them. `docs/UI.md` §5.5.2.
    #[test]
    fn a_sentence_too_long_for_the_box_is_cut_and_says_so() {
        let frame = whole_frame(&render_screen());
        let row = row_with(&frame, "Level 23 —");
        assert!(
            row.contains('…'),
            "the line was clipped in silence: {row:?}"
        );
        // And the cut leaves the border alone, unlike a raw clip.
        assert!(row.trim_end().ends_with('│'), "{row:?}");
        assert!(
            !row.contains("…│"),
            "the cut sits against the border: {row:?}"
        );
    }

    /// The selected row is drawn, and drawn **with colour and no glyph**: the box has
    /// no column to spare for a `▸`, and nothing in this list is bought, so the cursor
    /// only has to say where the player is.
    ///
    /// Without it the scroll would be invisible for the first screenful of presses —
    /// the cursor moves inside the window before the window has to move.
    #[test]
    fn the_selected_history_row_is_the_one_that_takes_the_accent() {
        let mut view = View::sample();
        view.stats.selected = 2;
        let buffer = render_view(&view);

        // The third row of the History box, whose inner area starts below the
        // `This run` panel and its own border.
        //
        // **The scan stops one column short of the inner area, and that column is the
        // scrollbar's.** The bar draws `█` in [`theme::ACCENT`] and `░` in
        // [`theme::MUTED`], so including it would make this test depend on whether the
        // thumb happens to cover the selected row: with the thumb there it passes for
        // the wrong reason, and with the track there it fails while the row is marked
        // perfectly well. The cursor is a property of the *text*, so the text is what
        // is scanned.
        let accented: Vec<String> = (0..buffer.area.height)
            .filter_map(|y| {
                let row: String = (30..80).map(|x| buffer[(x, y)].symbol()).collect();
                let coloured = (31..TEXT_END).all(|x| {
                    let cell = &buffer[(x, y)];
                    cell.symbol() == " " || cell.fg == theme::ACCENT
                });
                coloured.then_some(row)
            })
            .collect();

        assert!(
            accented.iter().any(|row| row.contains("Mine refilled")),
            "the third entry was not marked: {accented:?}"
        );
        assert_eq!(
            accented.len(),
            1,
            "more than one row claimed the cursor: {accented:?}"
        );
    }

    /// The History's scrollbar, top to bottom, as one string of `█` and `░`.
    ///
    /// Read off [`BAR_COLUMN`] rather than searched for across the frame the way the
    /// Levels screen searches for `█`: that screen has no other block glyph, and this
    /// one is a whole frame of three panels. Naming the column is what keeps this test
    /// about the bar rather than about whatever else might one day be drawn in a block.
    ///
    /// **Filtered to the two bar glyphs**, because the column runs the height of the
    /// *terminal* and the bar occupies only the History panel's inner rows: above and
    /// below it sit the two panels' borders (`─`) and the footer. Filtering is what lets
    /// a caller say "the thumb is at the top" without first working out which row the
    /// panel starts on — and it makes an absent bar the empty string, which is exactly
    /// the assertion a fitting log wants.
    fn bar_column(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| buffer[(BAR_COLUMN, y)].symbol())
            .filter(|symbol| matches!(*symbol, "█" | "░"))
            .collect()
    }

    /// The log outgrows its box long before a session is over — it is capped at 500 and
    /// every announcement enters it — so the panel says how much of it is off screen.
    #[test]
    fn the_history_draws_a_scrollbar_when_the_log_outgrows_the_box() {
        let bar = bar_column(&render_screen());
        assert!(bar.contains('█'), "no thumb in the bar column: {bar:?}");
        assert!(bar.contains('░'), "no track in the bar column: {bar:?}");
    }

    /// The thumb reports the **window**, not the cursor, which is the whole reason it is
    /// handed `range.start` and not a stored offset: `window` may move the box to keep
    /// the selection visible, and a bar drawn from anything else would report a scroll
    /// that did not happen.
    #[test]
    fn the_thumb_travels_as_the_history_is_scrolled() {
        let mut view = View::sample();
        let top = bar_column(&render_view(&view));
        assert!(
            top.starts_with('█') && top.ends_with('░'),
            "the thumb did not open at the top: {top:?}"
        );

        view.stats.selected = view.stats.history.len() - 1;
        let bottom = bar_column(&render_view(&view));
        assert!(
            bottom.ends_with('█') && bottom.starts_with('░'),
            "the thumb did not reach the bottom: {bottom:?}"
        );
    }

    /// **A fresh session pays for the column in blank space, not in a stuck thumb.**
    /// `scrollbar` draws nothing when the list fits, which is the state every run opens
    /// in — the log is empty until the first swing — and a full-height thumb there would
    /// read as a scrollbar that is broken rather than as one that has nothing to say.
    #[test]
    fn a_history_that_fits_its_box_draws_no_bar_at_all() {
        let mut view = View::sample();
        view.stats.history.truncate(2);
        view.stats.selected = 0;
        let bar = bar_column(&render_view(&view));
        assert!(bar.is_empty(), "a fitting log drew a bar: {bar:?}");
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
