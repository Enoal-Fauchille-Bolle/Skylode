//! The Upgrades screen — pickaxe, enchants, and both mine tracks (UI.md §5.4).
//!
//! The one screen that does not fit flat: ~96 rows of content cut into three
//! sub-tabs, each a list on the left and a detail pane on the right. The detail
//! pane exists so the tier-jump **dip** can be read *before* the purchase — a warning
//! you commit to, not one you discover.
//!
//! **The split is a single divider, not two abutting boxes.** Inventory and Mines
//! sit two panels side by side (`││`); here the frame draws one box fenced by a
//! `┬│┴` divider — at column 36 in the counted 80-column frame, and proportionally
//! further along as the terminal widens. Ratatui has no mid-box divider, so the
//! outer box is drawn and the divider's column is patched into the buffer by hand.

use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    action::Action,
    format::justified,
    screen::{panel, scrollbar},
    theme,
    view::{UpgradeSubtab, UpgradeTab, UpgradesView, View},
};

/// The master (list) side's share of the box, against [`DETAIL_WEIGHT`] — the
/// counted widths doubling as `Fill` weights, per the module note on `screen`.
///
/// Here the two sum to 77 rather than 80, because the divider takes a column of its
/// own and the box's two borders take the other two.
const LIST_WEIGHT: u16 = 35;

/// The detail pane's share of the box.
const DETAIL_WEIGHT: u16 = 42;

/// Draws the sub-tab bar, the master-detail box, and the footer.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    let upgrades = &view.upgrades;
    let [bar_area, box_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    subtab_bar(frame, bar_area, upgrades);

    let (list_area, detail_area) = master_detail(frame, box_area);
    let subtab = upgrades.active_subtab();
    list(frame, list_area, subtab);
    detail(frame, detail_area, subtab);

    frame.render_widget(
        Paragraph::new(subtab.footer.as_str()).style(Style::default().fg(theme::MUTED)),
        footer_area,
    );
}

/// The sub-tab bar: the three names with the active one bracketed, and the two
/// right-hand hints.
fn subtab_bar(frame: &mut Frame, area: Rect, upgrades: &UpgradesView) {
    let label = |tab: UpgradeTab| {
        let name = tab_name(tab);
        if tab == upgrades.active {
            format!("[{name}]")
        } else {
            format!(" {name} ")
        }
    };
    // Assembled in three plain pieces so the accented one can be located **by
    // construction**. Searching the finished string for its `[` would work today and
    // would be a trap the moment a label or a hint grew a bracket of its own.
    let mut before = " ".to_owned();
    let mut active = String::new();
    let mut after = String::new();
    for (index, tab) in [UpgradeTab::Pickaxe, UpgradeTab::Enchants, UpgradeTab::Mines]
        .into_iter()
        .enumerate()
    {
        let piece = if index == 0 {
            label(tab)
        } else {
            format!(" {}", label(tab))
        };
        if tab == upgrades.active {
            active = piece;
        } else if active.is_empty() {
            before.push_str(&piece);
        } else {
            after.push_str(&piece);
        }
    }

    // Justified on the assembled plain text, exactly as before: the padding is a
    // property of the whole row, so it is computed once, on the whole row.
    let line = justified(
        &format!("{before}{active}{after}"),
        "⇧←→  sub-tab           M  max ",
        area.width as usize,
    );
    // The tail is whatever `justified` added — the pad and the right-hand hints. Its
    // start is the column count of the three pieces, which is known rather than
    // searched for. `chars`, not bytes: `⇧←→` is multi-byte and one column each.
    let used = before.chars().count() + active.chars().count() + after.chars().count();
    let tail: String = line.chars().skip(used).collect();

    // Muted as a whole with the bracketed name lifted back out in the accent — the
    // top tab bar's relationship, one level down. The brackets stay: they are what
    // tells the active sub-tab apart once the hue is gone.
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw(before),
            Span::styled(active, Style::default().fg(theme::ACCENT)),
            Span::raw(after),
            Span::raw(tail),
        ]))
        .style(Style::default().fg(theme::MUTED)),
        area,
    );
}

/// The display name of a sub-tab.
fn tab_name(tab: UpgradeTab) -> &'static str {
    match tab {
        UpgradeTab::Pickaxe => "Pickaxe",
        UpgradeTab::Enchants => "Enchants",
        UpgradeTab::Mines => "Mines",
    }
}

/// Draws the outer box with a `┬│┴` divider and returns the (list, detail) rects.
///
/// The divider is patched straight into the buffer because ratatui draws borders
/// only around a `Block`, never through one: the outer box is rendered first, then
/// the divider's own column is overwritten with `│`, and the two border cells it
/// meets become `┬` and `┴`.
///
/// **The divider is a `Rect` from the layout, not an offset added to `inner.x`.**
/// Once the two sides became a ratio, "where is column 36" stopped being something
/// this function could compute — the solver decides it. Asking the layout for a
/// one-column strip and reading its `x` back is what keeps the patched glyphs on
/// the boundary the two panes actually meet at, at every terminal width.
fn master_detail(frame: &mut Frame, area: Rect) -> (Rect, Rect) {
    let block = panel("");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [list, divider, detail] = Layout::horizontal([
        Constraint::Fill(LIST_WEIGHT),
        Constraint::Length(1),
        Constraint::Fill(DETAIL_WEIGHT),
    ])
    .areas(inner);

    let divider_x = divider.x;
    let bottom = area.y + area.height.saturating_sub(1);
    // The divider is part of the box, so it has to be styled like the box. `panel`
    // colours its borders through `Block::border_style`, which never reaches these
    // hand-written cells — they are patched in *after* the block rendered — so the
    // style is set here explicitly or the divider draws in the default colour and
    // cuts a bright line through a muted frame.
    let border = Style::default().fg(theme::MUTED);
    let buffer = frame.buffer_mut();
    for y in inner.y..inner.y + inner.height {
        if let Some(cell) = buffer.cell_mut((divider_x, y)) {
            cell.set_symbol("│");
            cell.set_style(border);
        }
    }
    if let Some(cell) = buffer.cell_mut((divider_x, area.y)) {
        cell.set_symbol("┬");
        cell.set_style(border);
    }
    if let Some(cell) = buffer.cell_mut((divider_x, bottom)) {
        cell.set_symbol("┴");
        cell.set_style(border);
    }

    let list = Rect {
        x: inner.x,
        y: inner.y,
        width: LEFT_WIDTH,
        height: inner.height,
    };
    let detail = Rect {
        x: divider_x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(LEFT_WIDTH + 1),
        height: inner.height,
    };
    (list, detail)
}

/// The master list: the header rows, then the entries, with a scrollbar on the two
/// sub-tabs that overflow.
fn list(frame: &mut Frame, area: Rect, subtab: &UpgradeSubtab) {
    // Reserve the last column for a scrollbar only when the list scrolls; otherwise
    // the rows have the full width.
    let (rows_area, bar_area) = if subtab.scroll.is_some() {
        let [rows, bar] =
            Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        (rows, Some(bar))
    } else {
        (area, None)
    };

    let width = rows_area.width as usize;
    let mut lines: Vec<Line> = subtab
        .header
        .iter()
        .map(|h| Line::from(h.clone()).style(Style::default().fg(theme::MUTED)))
        .collect();
    for row in &subtab.rows {
        // Two mark channels: the cursor/current mark leads the row, the reachability
        // mark sits flush right where the eye scans a column of `✓ ~ ✗`.
        let lead = if row.cursor {
            " ▸ "
        } else if row.current {
            " ● "
        } else {
            "   "
        };
        // Flush right, so the reachability marks line up as a column and a long
        // row's own text can never crowd into its mark.
        // Both channels are coloured by the same pass, because both are marks: the
        // lead takes accent or magenta, the trailing `✓ ~ ✗` its reachability hue.
        let line = justified(&format!("{lead}{}", row.text), &row.mark, width);
        lines.push(theme::marked(&line));
    }
    frame.render_widget(Paragraph::new(lines), rows_area);

    // The scrollbar aligns with the rows, so it starts below the header — the
    // frame draws no thumb beside the column titles.
    if let (Some(bar_area), Some((total, position))) = (bar_area, subtab.scroll) {
        let offset = subtab.header.len() as u16;
        let bar = Rect {
            y: bar_area.y + offset,
            height: bar_area.height.saturating_sub(offset),
            ..bar_area
        };
        scrollbar(frame, bar, total, position);
    }
}

/// The detail pane: the selected entry's block of text, laid out in the fixture.
fn detail(frame: &mut Frame, area: Rect, subtab: &UpgradeSubtab) {
    // Through `marked` too: the dip block quotes the affordability of what is
    // selected, so the same `✓ ~ ✗` appear here as in the list beside it.
    let lines: Vec<Line> = subtab.detail.iter().map(|l| theme::marked(l)).collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// No contextual bindings yet; the sub-tab binding is configurable (§9).
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

    use super::*;

    /// Renders `view` through the Upgrades screen into an 80×24 buffer.
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

    /// Renders the screen with `tab` active.
    fn render_tab(tab: UpgradeTab) -> Buffer {
        let mut view = View::sample();
        view.upgrades.active = tab;
        render_view(&view)
    }

    fn sym(buffer: &Buffer, x: u16, y: u16) -> &str {
        buffer[(x, y)].symbol()
    }

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

    fn row_with<'a>(text: &'a str, needle: &str) -> &'a str {
        text.lines()
            .find(|line| line.contains(needle))
            .unwrap_or(text)
    }

    /// Just the list side (columns 0..36), joined per row — the detail pane repeats
    /// entry names (`Netherite Pickaxe`, `Obsidian`), so a list-only slice is what
    /// lets a test name a list row without matching the pane beside it.
    fn list_panel(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| (0..36).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_bar_shows_three_sub_tabs_with_the_active_one_bracketed() {
        let pickaxe = whole_frame(&render_tab(UpgradeTab::Pickaxe));
        let bar = row_with(&pickaxe, "Pickaxe");
        assert!(bar.contains("[Pickaxe]"), "{bar:?}");
        assert!(bar.contains("Enchants") && bar.contains("Mines"), "{bar:?}");
        assert!(
            bar.contains("⇧←→  sub-tab") && bar.contains("M  max"),
            "{bar:?}"
        );
        // Switching sub-tab moves the brackets, not the set of names.
        let enchants = whole_frame(&render_tab(UpgradeTab::Enchants));
        let bar = row_with(&enchants, "Pickaxe");
        assert!(bar.contains("[Enchants]"), "{bar:?}");
    }

    #[test]
    fn the_box_is_split_by_a_single_divider_at_column_thirty_six() {
        // One box, not two: a `┬` on the top border at column 36, `│` through the
        // body, `┴` on the bottom — the master-detail split this screen alone draws.
        let buffer = render_tab(UpgradeTab::Pickaxe);
        // The box sits between the sub-tab bar (row 0) and the footer (row 23), so
        // its bottom border is row 22, not the last row of the screen.
        assert_eq!(sym(&buffer, 36, 1), "┬", "no divider at the top border");
        assert_eq!(sym(&buffer, 36, 5), "│", "no divider through the body");
        assert_eq!(sym(&buffer, 36, 22), "┴", "no divider at the bottom border");
    }

    #[test]
    fn the_pickaxe_ladder_carries_both_mark_channels_and_its_detail() {
        let buffer = render_tab(UpgradeTab::Pickaxe);
        let list = list_panel(&buffer);
        let frame = whole_frame(&buffer);
        // The current rung is dotted, the cursor points, reachability ticks. The
        // marks are read off the list side — the detail pane names the same rung.
        assert!(row_with(&list, "Diamond Eff IV").contains('●'), "{list}");
        assert!(row_with(&list, "Netherite Pickaxe").contains('▸'), "{list}");
        assert!(
            frame.contains('✓') && frame.contains('~') && frame.contains('✗'),
            "{frame}"
        );
        // The detail pane, with the dip stated in ticks and its box art.
        assert!(frame.contains("tier jump"), "{frame}");
        assert!(frame.contains("Power  34.0 → 9.0"), "{frame}");
        assert!(frame.contains("27 → 100 ticks"), "{frame}");
    }

    #[test]
    fn the_reachability_marks_form_a_contiguous_tick_prefix() {
        // The ladder invariant: a cost cannot make an unaffordable chain affordable,
        // so once the marks leave `✓` they never return to it — `✓✓ ~ ✗✗`, never a
        // hole. Asserted on the fixture, the way the real ladder will be in phase 6.
        let rows = &View::sample().upgrades.pickaxe.rows;
        let mut left_ticks = false;
        for row in rows {
            match row.mark.as_str() {
                "✓" => assert!(!left_ticks, "a ✓ followed a non-✓: {:?}", row.text),
                "~" | "✗" => left_ticks = true,
                _ => {}
            }
        }
    }

    #[test]
    fn the_enchants_sub_tab_lists_its_tracks_and_needs_no_scrollbar() {
        let frame = whole_frame(&render_tab(UpgradeTab::Enchants));
        // `Level` is unique to the header — `Enchant` also hides in the bar's
        // `[Enchants]`, which would match the wrong row.
        assert!(row_with(&frame, "Level").contains("Cap"), "{frame}");
        assert!(
            frame.contains("Fortune") && frame.contains("Explosive"),
            "{frame}"
        );
        assert!(frame.contains("clears a 3x3 square"), "{frame}");
        // Six tracks fit in nineteen rows, so no thumb is drawn on this sub-tab.
        assert!(
            !frame.contains('█'),
            "the fitting sub-tab drew a scrollbar: {frame}"
        );
    }

    #[test]
    fn the_mines_sub_tab_repeats_each_mine_and_scrolls() {
        let frame = whole_frame(&render_tab(UpgradeTab::Mines));
        // `Track` is unique to the header — `Mine` also hides in the bar's `Mines`.
        assert!(row_with(&frame, "Track").contains("Next"), "{frame}");
        // The mine name repeats on both of its rows, so a scrolled row reads alone.
        assert!(frame.contains("Obsidian       Size"), "{frame}");
        assert!(
            row_with(&frame, "Obsidian       Richness").contains('▸'),
            "{frame}"
        );
        assert!(frame.contains("Ceiling   level 6 → 7"), "{frame}");
        // Twenty-four tracks overflow, so the thumb is drawn.
        assert!(
            frame.contains('█'),
            "the overflowing sub-tab drew no scrollbar: {frame}"
        );
    }

    #[test]
    fn each_sub_tab_names_its_own_purchase_in_the_footer() {
        let footer = |buffer: &Buffer| {
            (0..buffer.area.width)
                .map(|x| sym(buffer, x, 23))
                .collect::<String>()
        };
        assert!(footer(&render_tab(UpgradeTab::Pickaxe)).contains("buy to here"));
        assert!(footer(&render_tab(UpgradeTab::Pickaxe)).contains("buy max"));
        assert!(footer(&render_tab(UpgradeTab::Enchants)).contains("buy one level"));
        assert!(footer(&render_tab(UpgradeTab::Enchants)).contains("buy to cap"));
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
    fn both_mark_channels_keep_their_glyph_and_take_their_colour() {
        // The one screen with two mark columns at once: the lead mark on the left
        // and the reachability mark flush right. One `marked` pass colours both,
        // which is what keeps them from drifting apart.
        let buffer = render_tab(UpgradeTab::Pickaxe);
        assert_eq!(fg_of(&buffer, "●"), Some(theme::CURRENT));
        assert_eq!(fg_of(&buffer, "✓"), Some(theme::AFFORDABLE));
        assert_eq!(fg_of(&buffer, "✗"), Some(theme::REFUSED));
    }

    #[test]
    fn the_active_sub_tab_keeps_its_accent_over_the_bars_own_style() {
        // The bar sets a muted style on the whole `Paragraph` *and* an accent on one
        // span inside it. Which wins is a ratatui merging rule, not something this
        // screen controls — so it is asserted rather than assumed. Row 0, and the
        // `[` of the bracketed name is the first cell of the accented span.
        let buffer = render_tab(UpgradeTab::Enchants);
        let bracket = (0..buffer.area.width)
            .map(|x| &buffer[(x, 0)])
            .find(|cell| cell.symbol() == "[");
        assert_eq!(bracket.map(|cell| cell.fg), Some(theme::ACCENT));

        // And the inactive names really did take the muted style, or the accent
        // above would be distinguishing nothing.
        let inactive = (0..buffer.area.width)
            .map(|x| &buffer[(x, 0)])
            .find(|cell| cell.symbol() == "P");
        assert_eq!(inactive.map(|cell| cell.fg), Some(theme::MUTED));
    }

    #[test]
    fn the_hand_patched_divider_matches_the_border_it_joins() {
        // The divider is written straight into the buffer, so `Block::border_style`
        // never reaches it. Left unstyled it would cut a bright line down a muted
        // box — the one place in the crate where a colour can be forgotten silently.
        let buffer = render_tab(UpgradeTab::Pickaxe);
        assert_eq!(fg_of(&buffer, "┬"), Some(theme::MUTED));
        assert_eq!(fg_of(&buffer, "┴"), Some(theme::MUTED));
        assert_eq!(fg_of(&buffer, "╭"), Some(theme::MUTED), "the border moved");
    }
}
