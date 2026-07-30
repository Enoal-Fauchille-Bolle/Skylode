//! The Mines screen — pick a world and a mine, slide the richness dial (UI.md §5.2).
//!
//! Master-detail: a list of the twelve mines under three world headers on the left,
//! the selected mine's gate, size and richness on the right. Fifteen rows — twelve
//! mines plus three headers — fit in twenty, so this is the one list screen that
//! never needs a scrollbar at 80×24.
//!
//! The **richness dial is one control, drawn identically on all twelve mines** —
//! slider, arrows, rung, and the split beneath it. That is a departure from
//! UI-EN.md §5.3, which reserved the slider for the three mines whose two cells drop
//! *different* materials and replaced it with a flat readout on the other nine.
//!
//! The spec's argument was about the **stakes**, and it holds: only on Quartz,
//! Obsidian and the End is the setting a trade, since more Crying Obsidian is less
//! Obsidian. But the *control* is the same everywhere — on the nine others the dial
//! still decides what share of the grid is the dense block, worth nine of the ore
//! beside it, and the arrows still move it. A slider that appears on a quarter of
//! the screens is one the player has to learn twice, and hiding it would have left
//! nine mines' bought richness ceiling with no way to spend it, since raising the
//! ceiling and sliding the dial are two separate actions in the core.
//!
//! What differs per mine is therefore the **sentence under the dial**, not the
//! widget: `MineDetail::note` is where "this one has an optimum, not a maximum"
//! and "pure gain here" live. The split beneath the bar names the two **blocks**
//! rather than the two materials, because on those nine mines the materials are the
//! same word.

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use skylode_core::mine_kind::MineKind;
use skylode_core::world::World;

use crate::{
    action::Action,
    format::{justified, shown_rung},
    screen::panel,
    theme,
    view::{MineDetail, MineListRow, View},
};

/// The list panel's share of the row, against [`DETAIL_WEIGHT`] — and still the 38
/// columns UI-EN.md §5.3 counted, per the module note on `screen`.
const LIST_WEIGHT: u16 = 38;

/// The detail pane's share — the other 42 of the counted 80.
const DETAIL_WEIGHT: u16 = 42;

/// How many cells the dial bar spans between its `◄` and `►` arrows.
///
/// Twenty against the richness track's ten rungs, so a rung is exactly two cells and
/// the bar can be counted rather than estimated.
const DIAL_WIDTH: usize = 20;

/// The dial track's three regions, in the order they are drawn: where the dial sits,
/// what the bought ceiling leaves reachable above it, and what is not bought yet.
///
/// **Named because a test counts them**, and a test that searched for a literal `'·'`
/// would pass on a bar that had silently stopped drawing the other two. They are
/// `char` rather than `&str` so counting them in a rendered row is one `filter`.
const DIAL_FILLED: char = '█';
/// Bought and reachable, but above where the dial currently sits.
const DIAL_OWNED: char = '░';
/// Past the bought ceiling: a rung the player would have to buy before the dial can
/// reach it.
const DIAL_LOCKED: char = '·';

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
            // Two marks, and the cursor wins the column when they coincide: `▸` is
            // where the player is *looking*, which is the thing that moved and the
            // thing they need to find again. `●` is a standing fact they can recover
            // by walking the list. UI.md §5.8.5 keeps both a glyph and a colour, and
            // `theme::marked` derives the second from the first.
            let mark = if row.kind == view.mines.selected {
                " ▸ "
            } else if row.current {
                " ● "
            } else {
                "   "
            };
            let left = format!("{mark}{}", row.kind.name());
            lines.push(theme::marked(&justified(&left, &row_detail(row), width)));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// A list row's right-hand column: `8 x 5   R 6`, or the tier that still shuts it.
///
/// **Only the tier half of the lock is printed here.** A mine shut by its world is
/// shut for every mine in that world, and the world's own header row already carries
/// that level — repeating it on each of the three rows below would say one thing
/// four times. The End mine is the case that shows both at once: `Lv 30 ✗` on the
/// header, `locked   Netherite` on the row.
fn row_detail(row: &MineListRow) -> String {
    match row.lock.missing_tier() {
        Some(tier) => format!("locked   {}", tier.name()),
        None => format!(
            "{} x {}   R {}",
            row.size.0,
            row.size.1,
            shown_rung(row.richness_level)
        ),
    }
}

/// A run of `cells` copies of one track glyph.
///
/// [`str::repeat`] would need the glyph as a `&str`, and the three constants are
/// `char` so the tests can count them in a rendered row without slicing.
fn track(glyph: char, cells: usize) -> String {
    std::iter::repeat_n(glyph, cells).collect()
}

/// The pane's World row: `Nether        Lv 15  ✓`.
///
/// Composed rather than carried, which is the phase-4 change in one line: the world
/// and its threshold come from the mine's own kind, and the tick comes from the
/// [`MineLock`](skylode_core::mine_kind::MineLock) — so the `✓` cannot contradict
/// the level printed beside it, the way a stored string could.
///
/// The Overworld opens at level 1, which is where every run starts, so it prints no
/// threshold at all: `Lv 1` would name a gate nobody has ever been on the wrong side
/// of.
fn world_line(kind: MineKind, detail: &MineDetail) -> String {
    let world = kind.world();
    let tick = if detail.lock.missing_level().is_some() {
        "✗"
    } else {
        "✓"
    };
    if world.unlock_level() <= 1 {
        format!("{:<14}{tick}", world.name())
    } else {
        format!("{:<14}Lv {}  {tick}", world.name(), world.unlock_level())
    }
}

/// The pane's Gate row: `Diamond pickaxe      ✓`.
fn gate_line(kind: MineKind, detail: &MineDetail) -> String {
    let tick = if detail.lock.missing_tier().is_some() {
        "✗"
    } else {
        "✓"
    };
    format!(
        "{:<21}{tick}",
        format!("{} pickaxe", kind.gating_tier().name())
    )
}

/// The readout under the dial: the value cell's share on the left, the common
/// cell's flush right.
///
/// **The two blocks, not the two materials**, for the reason the pane's first line
/// names blocks: on the nine same-material mines the materials are the same word,
/// and this row would read `Iron 10%   Iron 90%`. The blocks never coincide, and
/// they are what the dial actually redraws — `Iron Block 10%   Iron Ore 90%` is the
/// composition the bar above is a picture of.
///
/// **A departure from the counted frame, and the same one §5.1 already records for
/// the Haul strip.** UI-EN.md §5.3 draws `Crying 64%   Obsidian 36%` indented eight
/// columns to sit under the bar — but it abbreviates, and the block is really
/// *Crying Obsidian*. Spelled out, that pair is 31 columns, and the indent plus a
/// readable gap does not fit the 38 this pane has. Rather than ship an abbreviation
/// table, the row loses the indent — landing in the label column every other row of
/// the pane already starts at — and is [`justified`], so the two shares sit at the
/// two edges however wide the pane is, and no longer a name can push them into each
/// other.
fn dial_split(kind: MineKind, detail: &MineDetail, width: usize) -> String {
    let value = detail.value_percent;
    // The complement, not a second reading: the two shares are one number and its
    // remainder, so a subtraction here is what stops them summing to 99 or 101.
    let common = 100_u32.saturating_sub(value);
    justified(
        &format!(" {} {value}%", kind.value_block().name()),
        &format!("{} {common}%", kind.common_block().name()),
        width,
    )
}

/// The columns a pane's inner area offers, as a [`usize`], minus the right margin
/// every row on this screen keeps clear.
fn width_of(inner: Rect) -> usize {
    (inner.width as usize).saturating_sub(LIST_MARGIN)
}

/// The detail pane: the selected mine's materials, gates, size, and the dial.
fn detail(frame: &mut Frame, area: Rect, view: &View) {
    let selected = view.mines.selected;
    let detail = &view.mines.detail;

    let block = panel(&format!(" {} Mine ", selected.name()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let (width, height) = (u32::from(detail.size.0), u32::from(detail.size.1));
    let total = width * height;
    let mut lines = vec![
        // The two **blocks**, not the two materials. On the three two-material mines
        // they read the same either way — `Obsidian  +  Crying Obsidian`, the frame's
        // own line — but on the nine others the materials are equal, so this line
        // would say `Stone  +  Stone`. The blocks never coincide, and they are the
        // more useful pair besides: `Iron Ore  +  Iron Block` is what the grid holds,
        // and the second is worth nine of the first.
        Line::from(format!(
            " {}  +  {}",
            selected.common_block().name(),
            selected.value_block().name(),
        )),
        Line::from(""),
        // Both carry a `✓`/`✗` of their own, so both go through `marked`.
        theme::marked(&format!(" World      {}", world_line(selected, detail))),
        theme::marked(&format!(" Gate       {}", gate_line(selected, detail))),
        Line::from(format!(
            " Size       {width} x {height} = {total}    level {}",
            shown_rung(detail.size_level),
        )),
        // `never entered` rather than `0 / 40`: a run creates its mines lazily, and
        // a zero here would claim the player had emptied one they have never opened.
        Line::from(match detail.blocks_standing {
            Some(standing) => format!(" Blocks     {standing} / {total}"),
            None => " Blocks     never entered".to_owned(),
        }),
        // **`ceiling`, not `level`.** This number is the highest rung the dial may be
        // pushed to, and the row below prints the rung the dial is *on* — two different
        // facts that both used to be spelled `x / y` with the word `level` in front of
        // one of them. `Ceiling` is the word the Upgrades pane already uses for this
        // exact track and the word `docs/MECHANICS.md` argues in ("buy the ceiling, set
        // the dial"), so the three places finally name it the same.
        Line::from(format!(
            " Richness   ceiling {}/{}",
            shown_rung(detail.richness_level),
            shown_rung(detail.richness_max),
        )),
        Line::from(""),
    ];

    // **One dial, drawn the same way on all twelve mines.** The slider used to be
    // reserved for the three whose two cells drop different materials, on the
    // argument that only there is the setting a *trade* worth picturing. It is drawn
    // everywhere now, because the argument was about the stakes and not about the
    // control: on the nine same-material mines the dial still decides what share of
    // the grid is the dense block, worth nine of the ore beside it, and the arrows
    // still move it. A slider that appears on a quarter of the screens is a control
    // the player has to learn twice.
    // **The bar is a picture of the number printed beside it**, and not of the grid's
    // composition. It used to be filled by `value_percent`, which is the honest reading
    // of a *different* question and made the control lie about its own ends: the curve
    // runs 10 % to 91 %, so the bottom rung showed a sliver and the top one stopped two
    // cells short — a slider that is neither empty when empty nor full when full. The
    // composition has not gone unsaid; it is the split line directly below, in absolute
    // percentages the bar cannot distort.
    //
    // Rungs *reached*, so rung 1 of 10 fills one tenth rather than nothing: the first
    // rung is a position on the ladder, not the absence of one. `DIAL_WIDTH` is 20
    // against 10 rungs, so each rung is exactly two cells and the bar is countable.
    //
    // **One scale, three regions.** The track is the run's ten rungs from end to end,
    // and the *bought ceiling* is where the owned part stops — so the number beside the
    // bar and the bar itself are both out of ten and cannot disagree. Printing the
    // ceiling as the denominator instead (`1/1` on a bar filled one tenth) put two
    // scales in one control, and the reading it invited — "I am at the maximum" — was
    // false about the only maximum that matters at the end of a run.
    //
    // A **texture** for the unbought tail rather than a marker glyph at the boundary: a
    // vertical rule on a slider track reads as the *handle*, and the handle here is
    // already the filled edge — at rung 3 of a bought 6 it would sit at 60 % while the
    // dial is at 30 %, making the loudest glyph on the row point at the wrong number.
    // Three glyphs and not three colours, for `docs/DECISIONS.md`'s reason about the
    // mine cells: colour discrimination is the unreliable channel, so the distinction
    // rides on the glyph and survives a remapped palette. At a maxed ceiling the tail is
    // empty and this is exactly the two-tone bar it has always been.
    let rungs = shown_rung(detail.richness_max) as usize;
    let cells = |rung: u32| (shown_rung(rung) as usize * DIAL_WIDTH) / rungs;
    let filled = cells(detail.richness_setting);
    let owned = cells(detail.richness_level);
    // Spans rather than `marked` here: the three track glyphs are not marks, and this
    // row is built by `format!` alone — no `justified` padding to preserve — so it can
    // be split safely. Same accent/muted pair as the gauges and the scrollbar, because
    // the dial is one more "how far along" bar.
    lines.push(Line::from(vec![
        Span::raw(" Dial   ◄ "),
        Span::styled(
            track(DIAL_FILLED, filled),
            Style::default().fg(theme::ACCENT),
        ),
        Span::styled(
            track(DIAL_OWNED, owned.saturating_sub(filled)),
            Style::default().fg(theme::MUTED),
        ),
        Span::styled(
            track(DIAL_LOCKED, DIAL_WIDTH.saturating_sub(owned)),
            Style::default().fg(theme::MUTED),
        ),
        Span::raw(" ►"),
        // The rung, after the arrow, counted from 1 like every other rung the player
        // reads, and against the run's ten — the same denominator the bar is drawn to.
        // What the pair no longer states is the ceiling; the bar's own boundary shows
        // that, and the `Ceiling` row above puts a number on it.
        // Five columns at the very most, against the six this row has spare.
        Span::styled(
            format!(
                "  {}/{}",
                shown_rung(detail.richness_setting),
                shown_rung(detail.richness_max)
            ),
            Style::default().fg(theme::MUTED),
        ),
    ]));
    lines.push(Line::from(dial_split(selected, detail, width_of(inner))));

    lines.push(Line::from(""));
    lines.push(Line::from("        free, reversible, any time"));
    lines.push(Line::from(""));
    for note in &detail.note {
        lines.push(Line::from(format!(" {note}")));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(" ← →  move the dial"));

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
    use skylode_core::pickaxe::PickaxeTier;

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
        assert!(row_with(&list, "Stone").contains("R 10"), "{list}");
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
        assert!(row_with(&frame, "Size").contains("level 4"), "{frame}");
        assert!(row_with(&frame, "Blocks").contains("31 / 40"), "{frame}");
        assert!(
            row_with(&frame, "Richness").contains("ceiling 7/10"),
            "{frame}"
        );
    }

    #[test]
    fn the_dial_is_drawn_for_a_two_material_mine() {
        let frame = whole_frame(&render_screen());
        // Obsidian is two-material, so the slider is drawn between its arrows.
        let dial = row_with(&frame, "Dial");
        assert!(dial.contains('◄') && dial.contains('►'), "{dial:?}");
        // The fixture's dial sits *on* its ceiling of 7/10, which is what a bought
        // ceiling normally does — so the middle region is empty here and the two the
        // bar draws are the filled part and the four unbought rungs above it.
        assert!(
            dial.contains(DIAL_FILLED) && dial.contains(DIAL_LOCKED),
            "{dial:?}"
        );
        assert!(frame.contains("optimum, not a maximum."), "{frame}");
    }

    /// The three counts, at the position that used to be drawn as a lie.
    ///
    /// Every part of this is one scale — the run's ten rungs, end to end — which is the
    /// point: the number beside the bar is out of ten too, so the two cannot disagree.
    /// The old bar printed the *ceiling* as the denominator, so a fresh mine read `1/1`
    /// beside a bar filled one tenth and invited exactly the wrong conclusion.
    ///
    /// [`DIAL_WIDTH`] = 20 over ten rungs makes a rung two cells, so these are exact
    /// counts rather than approximately right ones.
    #[test]
    fn the_dial_track_separates_where_it_sits_from_what_is_bought() {
        fn regions(setting: u32, ceiling: u32) -> (usize, usize, usize) {
            let mut view = View::sample();
            view.mines.detail.richness_setting = setting;
            view.mines.detail.richness_level = ceiling;
            let frame = whole_frame(&render_view(&view));
            let dial = row_with(&frame, "Dial");
            let count = |glyph: char| dial.chars().filter(|&c| c == glyph).count();
            (count(DIAL_FILLED), count(DIAL_OWNED), count(DIAL_LOCKED))
        }

        // A fresh mine: rung 1 of 10, and nine rungs it has not bought. One tenth
        // filled and not nothing — the first rung is a position on the ladder, and
        // `1/10` beside an empty bar would contradict itself.
        assert_eq!(regions(0, 0), (2, 0, 18), "a fresh mine");
        // Rung 4 of a bought 7: the middle region is the headroom already paid for,
        // and it is the whole reason the ceiling needs no number on this row.
        assert_eq!(regions(3, 6), (8, 6, 6), "room bought above the dial");
        // A maxed ceiling leaves no tail, so the bar is the two-tone one it always was.
        assert_eq!(regions(4, 9), (10, 10, 0), "the halfway rung reads half");
        assert_eq!(regions(9, 9), (DIAL_WIDTH, 0, 0), "the last rung");
    }

    /// The two shares are the dial's own number and its remainder, spelled with the
    /// materials' real names.
    ///
    /// The counted frame writes `Crying 64%   Obsidian 36%`, abbreviating a material
    /// that is really *Crying Obsidian*; the row is justified across the pane
    /// instead, so this asserts the two ends and the sum rather than a fixed gap that
    /// would only hold at 80 columns.
    #[test]
    fn the_split_under_the_dial_names_both_materials_and_sums_to_a_hundred() {
        let frame = whole_frame(&render_screen());
        // Matched on the percentage, not on the material: `Crying Obsidian` also
        // appears two rows up, in the pane's own `Obsidian  +  Crying Obsidian`.
        let split = row_with(&frame, "64%");
        assert!(split.contains("Crying Obsidian 64%"), "{frame}");
        // Positions rather than `ends_with`: this row is a slice of the whole 80-column
        // frame, so it ends in the pane's border and the list beside it, not in the
        // text under test. The gap is what proves the row was justified — the two
        // shares abutted before it was, which is the bug this test was written for.
        const VALUE: &str = "Crying Obsidian 64%";
        let gap = match (split.find(VALUE), split.rfind("Obsidian 36%")) {
            (Some(value_at), Some(common_at)) => common_at.saturating_sub(value_at + VALUE.len()),
            // A missing share is a zero gap, so the one assertion below covers both
            // failures — the crate's lints leave no `panic!` to separate them with.
            _ => 0,
        };
        assert!(gap >= 2, "the two shares abut or are missing: {split:?}");

        // The two are one number and its complement, so they are read back off the
        // rendered row and added: a change that made the pane compute the second
        // share independently would show up as a pair that no longer sums to 100,
        // which two `contains` of fixed strings could never catch.
        let shares: Vec<u32> = split
            .split('%')
            .filter_map(|chunk| chunk.rsplit(' ').next()?.parse().ok())
            .collect();
        assert_eq!(shares.len(), 2, "not two shares on {split:?}");
        assert_eq!(shares.iter().sum::<u32>(), 100, "{split:?}");
    }

    /// A same-material mine gets the same slider, and a split that is readable.
    ///
    /// Iron's two *materials* are both `Iron`, so a split built from them would read
    /// `Iron 10%   Iron 90%` and say nothing. It is built from the two **blocks**,
    /// which never coincide — and on this mine that is the whole point of the dial:
    /// the value cell is worth nine of the common one.
    #[test]
    fn a_same_material_mine_gets_the_same_slider_and_a_split_that_names_both_blocks() {
        let mut view = View::sample();
        view.mines.selected = MineKind::Iron;
        let frame = whole_frame(&render_view(&view));

        let dial = row_with(&frame, "Dial");
        assert!(dial.contains('◄') && dial.contains('►'), "{dial:?}");
        assert!(
            dial.contains(DIAL_FILLED) && dial.contains(DIAL_LOCKED),
            "{dial:?}"
        );

        // Matched on the percentage: `Iron Block` also appears three rows up, in the
        // pane's own `Iron Ore  +  Iron Block`.
        let split = row_with(&frame, "64%");
        assert!(split.contains("Iron Block 64%"), "{frame}");
        assert!(split.contains("Iron Ore 36%"), "{frame}");
        assert!(frame.contains("← →  move the dial"), "{frame}");
    }

    /// The rung is printed after the arrow, **against the run's ten and not against the
    /// ceiling** — the same denominator the bar is drawn to.
    ///
    /// Against the ceiling it read `1/1` on a fresh mine, beside a bar filled one tenth:
    /// two scales in one control, and the reading it invited ("I am at the maximum") was
    /// false about the only maximum that matters by the end of a run. The ceiling is not
    /// lost — the bar stops drawing owned track where it ends, and the `Ceiling` row
    /// above puts the number on it.
    ///
    /// A dial at rung 4 of a bought 7 is the case that tells the two apart: `4/7` would
    /// pass here under the old rule and `4/10` under the new one.
    #[test]
    fn the_dial_prints_its_rung_against_the_runs_ten_rungs() {
        let mut view = View::sample();
        view.mines.detail.richness_setting = 3;
        view.mines.detail.richness_level = 6;
        let frame = whole_frame(&render_view(&view));

        let dial = row_with(&frame, "Dial");
        assert!(dial.contains("4/10"), "{dial:?}");
        assert!(
            !dial.contains("4/7"),
            "the ceiling is still the denominator"
        );
    }

    /// The pane's two gate rows are the two-axis lock drawn whole.
    ///
    /// The End mine is the one row in the fixture where both axes are shut — Lv 30
    /// against a level-23 save, and Netherite against a Diamond pickaxe — so it is
    /// the only selection that puts a `✗` on *both* lines. Selecting it is what
    /// proves the ticks are derived from the lock rather than transcribed: the
    /// Obsidian pane the frame draws would pass with two hardcoded `✓`.
    #[test]
    fn a_mine_shut_on_both_axes_marks_both_gate_rows() {
        let mut view = View::sample();
        // Both halves move together: `selected` names the mine and `detail` is that
        // mine's, so a test that changed only one would be describing a screen the
        // projection cannot produce.
        view.mines.selected = MineKind::Amethyst;
        view.mines.detail.lock = MineKind::Amethyst.lock(23, PickaxeTier::Diamond);
        let frame = whole_frame(&render_view(&view));

        let world = row_with(&frame, "World ");
        assert!(world.contains("Lv 30") && world.contains('✗'), "{world:?}");
        let gate = row_with(&frame, "Gate ");
        assert!(
            gate.contains("Netherite pickaxe") && gate.contains('✗'),
            "{gate:?}"
        );
    }

    /// A mine this run has never entered has no grid to count, and says so.
    ///
    /// `0 / 40` would claim the player had emptied a mine they have never opened —
    /// which is why the count is an `Option` in the view rather than a number with a
    /// convention attached to its zero.
    #[test]
    fn a_mine_never_entered_counts_no_blocks() {
        let mut view = View::sample();
        view.mines.detail.blocks_standing = None;
        let frame = whole_frame(&render_view(&view));

        assert!(
            row_with(&frame, "Blocks").contains("never entered"),
            "{frame}"
        );
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

    /// All three track regions, and the pair every other bar on every other screen is
    /// drawn in.
    ///
    /// A dial below its ceiling, because the fixture's is *on* its ceiling and would
    /// leave the middle region — the one this test exists to colour — with no cells to
    /// find. The two muted regions share a colour on purpose: they are told apart by
    /// their glyph, which is the channel `docs/DECISIONS.md` argues is the reliable one,
    /// and inventing a third tone would need a contrast gate against a background this
    /// process cannot see (see `theme`).
    #[test]
    fn the_dial_is_drawn_in_the_same_pair_as_every_other_progress_bar() {
        let mut view = View::sample();
        view.mines.detail.richness_setting = 3;
        view.mines.detail.richness_level = 6;
        let buffer = render_view(&view);

        assert_eq!(fg_of(&buffer, "█"), Some(theme::ACCENT));
        assert_eq!(fg_of(&buffer, "░"), Some(theme::MUTED));
        assert_eq!(fg_of(&buffer, "·"), Some(theme::MUTED));
    }
}
