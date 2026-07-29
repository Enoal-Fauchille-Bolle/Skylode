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
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

use skylode_core::economy::CostLine;

use crate::{
    action::Action,
    cursor::{MineTrack, UpgradeTab},
    format::{MAXED, grouped, justified},
    screen::{panel, scrollbar, window},
    theme,
    view::{
        EnchantDetail, MineTrackDetail, NOTHING, PickaxeDetail, TrackBlock, TrackOutcome,
        UpgradeDetail, UpgradeSubtab, UpgradesView, View, level_word,
    },
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
    for (index, tab) in UpgradeTab::ALL.into_iter().enumerate() {
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

    (list, detail)
}

/// The width of each column, measured over the header and every row.
///
/// **The alignment lives here and not in the read model**, which is phase 6's
/// instance of the lesson phases 4 and 5 both learned: a projection that pads its own
/// strings has decided a layout without knowing the pane's width, and a name one
/// character longer breaks every row under it. Measuring is cheap — forty-six rows at
/// worst, once per redraw — and it is right by construction.
fn columns(subtab: &UpgradeSubtab) -> Vec<usize> {
    let mut widths: Vec<usize> = Vec::new();
    let rows = std::iter::once(&subtab.header).chain(subtab.rows.iter().map(|row| &row.cells));
    for cells in rows {
        for (index, cell) in cells.iter().enumerate() {
            let width = cell.chars().count();
            match widths.get_mut(index) {
                Some(current) => *current = (*current).max(width),
                None => widths.push(width),
            }
        }
    }
    widths
}

/// Lays `cells` out in `widths`, two spaces between columns and none after the last.
fn columned(cells: &[String], widths: &[usize]) -> String {
    let mut out = String::new();
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            out.push_str(COLUMN_GAP);
        }
        out.push_str(cell);
        // The final column is not padded: a trailing run of spaces would push the
        // flush-right mark off the edge on a narrow pane for no visible gain.
        if index + 1 < cells.len() {
            let pad = widths.get(index).copied().unwrap_or(0);
            for _ in cell.chars().count()..pad {
                out.push(' ');
            }
        }
    }
    out
}

/// The gap between two columns of a sub-tab's table.
///
/// **One space, and it is a budget rather than a taste.** The widest Mines row is
/// `Ancient Debris` (14) + `Richness` (8) + `Lv 30` (5), which with the three-column
/// lead mark already spends 30 of the list pane's 34 — at two spaces it spends 34, and
/// the flush-right reachability mark has nowhere left to go. A table that silently
/// drops the column it exists to show is worse than a tight one.
const COLUMN_GAP: &str = " ";

/// The blank columns between the flush-right reachability mark and the scrollbar.
///
/// **One, and it is a budget rather than a taste** — the same argument
/// [`COLUMN_GAP`] makes, against the same row. The §5.4 Pickaxe frame draws two here
/// and the §5.4.2 Mines frame draws none, which is a contradiction inside one section
/// of `docs/UI.md`; the arithmetic settles it. The widest Mines row is the lead
/// mark (3) + `Ancient Debris` (14) + a gap + `Richness` (8) + a gap + `Lv 30` (5) =
/// 32 columns, against the 34 the list pane has once the bar column is reserved. One
/// gutter plus one mark leaves exactly 32; two would leave 31 and push the mark column
/// off the pane — the very failure `COLUMN_GAP` was cut to one space to avoid.
const MARK_GUTTER: usize = 1;

/// The master list: the header row, then the entries, with a scrollbar on the two
/// sub-tabs that overflow.
fn list(frame: &mut Frame, area: Rect, subtab: &UpgradeSubtab) {
    // How many rows fit, read off the `Rect` rather than off the view. Reserving the
    // scrollbar column narrows the rows but not their number, so the count is taken
    // first and stands.
    let header_rows = u16::from(!subtab.header.is_empty());
    let visible = usize::from(area.height.saturating_sub(header_rows));
    let range = window(subtab.rows.len(), subtab.cursor(), subtab.offset, visible);

    // **The bar column is reserved on every sub-tab, and drawn on the two that
    // overflow.** Reserving it only when the list scrolls made the mark column jump one
    // column between Pickaxe and Enchants — a column of glyphs that moves when the
    // player changes sub-tab is the one thing a column of glyphs must not do.
    // `screen::scrollbar` draws nothing when the list fits, so the reservation costs a
    // blank column and never a stuck thumb.
    let [rows_area, bar_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(area);

    let width = (rows_area.width as usize).saturating_sub(MARK_GUTTER);
    let widths = columns(subtab);
    let mut lines: Vec<Line> = Vec::new();
    if !subtab.header.is_empty() {
        // Indented by the lead-mark column, so a title sits over its own cells.
        lines.push(
            Line::from(format!("   {}", columned(&subtab.header, &widths)))
                .style(Style::default().fg(theme::MUTED)),
        );
    }
    for row in &subtab.rows[range.clone()] {
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
        let line = justified(
            &format!("{lead}{}", columned(&row.cells, &widths)),
            row.mark.glyph(),
            width,
        );
        lines.push(theme::marked(&line));
    }
    frame.render_widget(Paragraph::new(lines), rows_area);

    // The scrollbar aligns with the rows, so it starts below the header — the
    // frame draws no thumb beside the column titles. Its position is the window's
    // own start, not the view's stored offset: `window` may have moved it to keep
    // the cursor on screen, and a thumb pointing at where the list *used* to be
    // would be reporting a scroll that did not happen.
    let bar = Rect {
        y: bar_area.y + header_rows,
        height: bar_area.height.saturating_sub(header_rows),
        ..bar_area
    };
    scrollbar(frame, bar, subtab.rows.len(), visible, range.start);
}

/// The detail pane: the selected row, described.
///
/// **The pane is composed here, from typed data** — it used to be a `Vec<String>` the
/// view handed over whole, which was honest while every number in it was a
/// placeholder and impossible once they became real.
fn detail(frame: &mut Frame, area: Rect, subtab: &UpgradeSubtab) {
    let text = match &subtab.detail {
        UpgradeDetail::Pickaxe(detail) => pickaxe_pane(detail),
        UpgradeDetail::Enchant(detail) => enchant_pane(detail),
        UpgradeDetail::Mine(detail) => mine_pane(detail),
    };
    // Through `marked`: the pane quotes the affordability of what is selected, so the
    // same `✓ ~ ✗` appear here as in the list beside it, in the same hues.
    let lines: Vec<Line> = text.iter().map(|line| theme::marked(line)).collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// A labelled block: the label once, then one line per value, the rest indented under
/// it — the shape every pane in §5.4 repeats.
fn block(label: &str, values: &[String]) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if index == 0 {
            lines.push(format!(" {label:<9} {value}"));
        } else {
            lines.push(format!("           {value}"));
        }
    }
    lines
}

/// A cost line as the panes quote it: `2 Compressed Diamond + 60 Ancient Debris`, one
/// denomination per entry so a price is never rounded into the wrong shape.
fn cost_lines(costs: &[CostLine]) -> Vec<String> {
    costs
        .iter()
        .flat_map(CostLine::requirements)
        .map(|(item, amount)| format!("{} {item}", grouped(amount)))
        .collect()
}

/// The Pickaxe pane (UI.md §5.4).
fn pickaxe_pane(detail: &PickaxeDetail) -> Vec<String> {
    let mut lines = vec![justified(
        &format!(" {}", detail.title),
        if detail.crosses_tier_jump {
            "tier jump "
        } else {
            ""
        },
        DETAIL_WIDTH,
    )];
    lines.push(String::new());

    if detail.chain.is_empty() {
        lines.push(" Owned already — nothing to buy here.".to_owned());
        return lines;
    }

    let rungs = if detail.chain.len() == 1 {
        "1 rung".to_owned()
    } else {
        format!("{} rungs", detail.chain.len())
    };
    lines.push(justified(
        &format!(" {:<9} {rungs}", "Chain"),
        &format!("{} ", detail.mark.glyph()),
        DETAIL_WIDTH,
    ));
    lines.extend(block("Cost", &cost_lines(&detail.costs)));

    if let Some(dip) = &detail.dip {
        lines.push(String::new());
        lines.push(" ┌────────────────────────────────────┐".to_owned());
        lines.push(boxed(&format!(
            "Power  {:.1} → {:.1}",
            dip.power_before, dip.power_after
        )));
        lines.push(boxed(&format!(
            "{}  {} → {} ticks",
            dip.block.name(),
            ticks(dip.ticks_before),
            ticks(dip.ticks_after)
        )));
        if let Some(repaid) = &dip.repaid_at {
            lines.push(boxed(&format!(
                "Repaid at {} ({:.1})",
                repaid.rung, repaid.power
            )));
        }
        lines.push(" └────────────────────────────────────┘".to_owned());
    }

    if !detail.unlocks.is_empty() {
        lines.push(String::new());
        let names: Vec<String> = detail
            .unlocks
            .iter()
            .map(|kind| format!("the {} mine", kind.name()))
            .collect();
        lines.extend(block("Unlocks", &names));
    }
    if let Some((before, after)) = detail.ceiling {
        lines.push(String::new());
        lines.extend(block(
            "Ceiling",
            &[format!("Efficiency {before} → {after}")],
        ));
    }
    lines
}

/// One line inside the dip box's art.
fn boxed(text: &str) -> String {
    format!(" │ {text:<34} │")
}

/// A tick count, or the em dash for a pickaxe that would never break the block.
///
/// [`None`] is unreachable through a real pickaxe — every tier's base power is above
/// zero — and it reads `—` rather than a number for the same reason the empty gauges
/// do: a count would assert that the block eventually falls.
fn ticks(count: Option<u32>) -> String {
    count.map_or_else(|| NOTHING.to_owned(), grouped)
}

/// The Enchants pane (UI.md §5.4.1).
fn enchant_pane(detail: &EnchantDetail) -> Vec<String> {
    let mut lines = vec![
        justified(
            &format!(" {}", detail.kind.name()),
            &format!("level {} ", level_word(detail.level)),
            DETAIL_WIDTH,
        ),
        String::new(),
    ];
    lines.extend(block("Effect", &detail.effect));
    lines.push(String::new());
    lines.extend(block("Next", &detail.next));
    lines.push(String::new());

    match &detail.cost {
        Some(cost) => {
            let quoted = cost_lines(cost);
            lines.extend(block("Cost", &quoted));
            // The mark goes on the *last* cost line, where the eye lands after
            // reading the price — the frame's own placement.
            if let Some(last) = lines.last_mut() {
                *last = justified(last, &format!("{} ", detail.mark.glyph()), DETAIL_WIDTH);
            }
        }
        None => lines.extend(block("Cost", &["nothing left to buy".to_owned()])),
    }

    lines.push(String::new());
    lines.extend(block(
        "Cap",
        &[
            format!("{} — the world's, and one", detail.cap),
            "number for all five specials".to_owned(),
        ],
    ));
    lines
}

/// The Mines pane (UI.md §5.4.2).
///
/// **Four lines of it are spent refusing one conflation**, and that is the frame's own
/// choice: richness is the only word in the game that appears next to a price *and*
/// next to a free cursor, and this is the one place both senses are on screen at once.
fn mine_pane(detail: &MineTrackDetail) -> Vec<String> {
    let track = match detail.track {
        MineTrack::Size => "size",
        MineTrack::Richness => "richness",
    };
    let mut lines = vec![
        format!(" {} Mine — {track}", detail.kind.name()),
        String::new(),
    ];

    if let Some(blocked) = detail.blocked {
        lines.extend(match blocked {
            // **Two independent `if`s, not a match on the pair**, mirroring the shape
            // `MineLock` itself chose: the two axes are independent, so a match would
            // need a fourth arm for the both-open case that no locked track can be in.
            // Built as a list, the sentence reads the same and there is no arm the
            // tests can only reach by fabricating a lock the projection never makes.
            TrackBlock::Locked(lock) => {
                let mut needs = Vec::new();
                if let Some(level) = lock.missing_level() {
                    needs.push(format!("needs level {level}"));
                }
                if let Some(tier) = lock.missing_tier() {
                    let lead = if needs.is_empty() { "needs" } else { "and" };
                    needs.push(format!("{lead} a {} pickaxe", tier.name()));
                }
                block("Locked", &needs)
            }
            TrackBlock::NotEntered => block(
                "Not yet",
                &[
                    "this run has never opened it.".to_owned(),
                    "Enter it once from 2 Mines and".to_owned(),
                    "its tracks open.".to_owned(),
                ],
            ),
        });
        return lines;
    }

    let (level, next) = detail.level;
    lines.extend(block(
        match detail.track {
            MineTrack::Size => "Size",
            MineTrack::Richness => "Ceiling",
        },
        &[match detail.at_next {
            TrackOutcome::Maxed => format!("level {level} — {MAXED}"),
            _ => format!("level {level} → {next}"),
        }],
    ));

    match detail.at_next {
        TrackOutcome::Size((width, height)) => {
            lines.extend(block(
                &format!("At {next}"),
                &[format!("{width}x{height} cells")],
            ));
        }
        TrackOutcome::Richness(percent) => {
            lines.extend(block("Dial", &["free, on the Mines screen".to_owned()]));
            lines.push(String::new());
            lines.extend(block(
                &format!("At {next}"),
                &[format!("{} {percent}%", detail.kind.value_block().name())],
            ));
        }
        TrackOutcome::Maxed => {}
    }

    lines.push(String::new());
    match &detail.cost {
        Some(cost) => {
            lines.extend(block("Cost", &cost_lines(cost)));
            lines.push(String::new());
            // **Two lines per material, the name then the amounts.** One line reads
            // better and does not fit: `0 Compressed Crying Obsidian, 2 raw` is 35
            // columns against the 31 a labelled block leaves, so it would be cut off
            // exactly where the number the player came to read sits.
            let held: Vec<String> = detail
                .held
                .iter()
                .flat_map(|(material, compressed, raw)| {
                    [
                        material.name().to_owned(),
                        format!("{} Compressed, {} raw", grouped(*compressed), grouped(*raw)),
                    ]
                })
                .collect();
            lines.extend(block("You hold", &held));
            if let Some(last) = lines.last_mut() {
                *last = justified(last, &format!("{} ", detail.mark.glyph()), DETAIL_WIDTH);
            }
        }
        None => lines.extend(block("Cost", &["nothing left to buy".to_owned()])),
    }

    if detail.track == MineTrack::Richness {
        lines.push(String::new());
        lines.push(" This buys the ceiling only. The".to_owned());
        lines.push(" dial slides anywhere at or below".to_owned());
        lines.push(" it, free and reversible, on the".to_owned());
        lines.push(" Mines screen.".to_owned());
    }
    lines
}

/// The width the panes justify their right-hand marks against.
///
/// The counted frame's detail pane, and a plain constant rather than the `Rect`'s own
/// width: these lines are prose the wireframes were drawn around, so widening the
/// terminal should give the pane more room, not stretch a `tier jump` tag to the far
/// edge of a 200-column screen.
const DETAIL_WIDTH: usize = DETAIL_WEIGHT as usize;

/// `↑↓` walk the rows, `Enter` buys to the cursor, `M` buys as far as the ore goes.
///
/// **`←/→` is deliberately absent** (UI.md §9): the richness *dial* is never set here
/// — this screen buys the ceiling — so the lateral axis is left free for the
/// configurable sub-tab binding, which [`crate::keymap`] resolves before this
/// function is reached because it is the only place that can see the config.
///
/// `M` and not `m`: it spends an inventory, and the shifted key is one the hand does
/// not reach for by accident.
pub fn map_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Up => Some(Action::CursorUp),
        KeyCode::Down => Some(Action::CursorDown),
        KeyCode::Enter => Some(Action::Confirm),
        KeyCode::Char('M') => Some(Action::BuyMax),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};
    use skylode_core::{
        enchant::EnchantType, game::GameState, material::Material, mine_kind::MineKind,
    };

    use super::*;
    use crate::view::Mark;

    /// Renders `view` through the Upgrades screen into an 80×24 buffer.
    fn render_view(view: &View) -> Buffer {
        render_view_sized(view, 80, 24)
    }

    /// The same, at an arbitrary size — for the responsive assertions.
    fn render_view_sized(view: &View, width: u16, height: u16) -> Buffer {
        let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
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

    /// The first inked column of the detail-pane row carrying `label`, and its colour,
    /// searched from `from` — the pane's own left edge for a label, past
    /// [`LABEL_COLUMNS`] for the value beside it.
    ///
    /// Anchored on the row rather than on a glyph: the panes repeat every letter of
    /// the alphabet, so a buffer-wide scan for `C` finds whichever `Cobblestone` came
    /// first and reports a colour about the wrong thing.
    fn ink(buffer: &Buffer, label: &str, from: u16) -> Option<Color> {
        let frame = whole_frame(buffer);
        let y = u16::try_from(frame.lines().position(|line| line.contains(label))?).ok()?;
        (from..buffer.area.width)
            .find(|&x| sym(buffer, x, y) != " ")
            .map(|x| buffer[(x, y)].fg)
    }

    /// The detail pane's left edge: one past the divider the frame draws at 36.
    const PANE_X: u16 = 37;

    #[test]
    fn a_price_is_drawn_in_the_hue_of_its_own_verdict() {
        // The colour doubles the `✗` further down the pane, so removing it loses
        // nothing the pane did not already say — which is the whole of §4.5's rule.
        // What it buys is that a refused price reads as refused at a glance rather than
        // after the price has been read.
        let buffer = render_tab(UpgradeTab::Mines);
        assert_eq!(
            ink(&buffer, "Cost", PANE_X + LABEL_COLUMNS as u16),
            Some(theme::REFUSED),
            "the price is not tinted with its verdict:\n{}",
            whole_frame(&buffer)
        );
        // And `You hold` beside it is not: the tint is the *price*'s reading, so
        // spreading it over the pane would make it decoration.
        assert_eq!(
            ink(&buffer, "You hold", PANE_X + LABEL_COLUMNS as u16),
            Some(Color::Reset),
            "the tint leaked past the price"
        );
    }

    #[test]
    fn a_blocks_label_is_muted_and_its_value_is_not() {
        // Labels are chrome — the same role `MUTED` already plays for table headers —
        // so the eye lands on the answer rather than on the word introducing it.
        let buffer = render_tab(UpgradeTab::Pickaxe);
        assert_eq!(
            ink(&buffer, "Chain", PANE_X),
            Some(theme::MUTED),
            "the `Chain` label kept the default foreground:\n{}",
            whole_frame(&buffer)
        );
        assert_eq!(
            ink(&buffer, "Chain", PANE_X + LABEL_COLUMNS as u16),
            Some(Color::Reset),
            "the muting ran past the label"
        );
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
        // hole. Asserted here on the fixture; `view` asserts the same of a real run.
        let rows = &View::sample().upgrades.pickaxe.rows;
        let mut left_ticks = false;
        for row in rows {
            match row.mark {
                Mark::Affordable => {
                    assert!(!left_ticks, "a ✓ followed a non-✓: {:?}", row.cells)
                }
                Mark::CompressFirst | Mark::Refused => left_ticks = true,
                Mark::Owned | Mark::NoPrice => {}
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

    /// How many of the ladder's rungs the buffer actually shows.
    ///
    /// Counted **per row of the list panel**, not by searching the panel for each
    /// rung's label: every rung label is a prefix of the next (`Netherite Eff I`
    /// lives inside `Netherite Eff II`), so a `contains` per label overcounts. Every
    /// rung says either `Pickaxe` or ` Eff `, so one match per drawn row is exact —
    /// **once the rows outside the box are excluded**. The sub-tab bar's own
    /// `[Pickaxe]` sits on row 0 and would otherwise count as a rung; a row of the
    /// list is one that opens on the box's left border.
    fn rungs_drawn(buffer: &Buffer) -> usize {
        list_panel(buffer)
            .lines()
            .filter(|line| line.starts_with('│'))
            .filter(|line| line.contains("Pickaxe") || line.contains(" Eff "))
            .count()
    }

    #[test]
    fn a_taller_terminal_shows_more_of_the_ladder() {
        // The screenshot's complaint, as an assertion. The ladder is 46 rungs; at
        // 80×24 twenty of them fit, and the rest of the box used to stay empty
        // however much room the terminal had, because the view carried twenty rows
        // and no more.
        let view = View::sample();
        let counted = rungs_drawn(&render_view_sized(&view, 80, 24));
        let tall = rungs_drawn(&render_view_sized(&view, 80, 48));
        assert_eq!(counted, 20, "the counted frame no longer shows 20 rungs");
        assert!(
            tall > counted,
            "a 48-row terminal drew {tall} rungs, no more than the {counted} at 24"
        );
        // And never more than exist — a window past the end of the list would be a
        // panic, so its absence is worth stating.
        assert!(tall <= view.upgrades.pickaxe.rows.len());
    }

    #[test]
    fn a_tall_enough_terminal_shows_the_whole_ladder_and_drops_the_scrollbar() {
        // 46 rungs plus the chrome fit in 48 rows, so nothing is cut off — and the
        // scrollbar goes away, because whether one is drawn is now `rows > visible`
        // rather than a flag the fixture set once.
        let view = View::sample();
        let buffer = render_view_sized(&view, 80, 50);
        assert_eq!(rungs_drawn(&buffer), view.upgrades.pickaxe.rows.len());
        assert!(
            !whole_frame(&buffer).contains('█'),
            "a list that fits still drew a scrollbar"
        );
    }

    #[test]
    fn a_wider_terminal_widens_both_panes_and_moves_the_divider_with_them() {
        // The other half of the screenshot: the list stayed 35 columns while the
        // detail pane took every spare one. Now the divider sits proportionally,
        // which is the visible proof that both sides grew.
        let view = View::sample();
        let divider = |width: u16| {
            let buffer = render_view_sized(&view, width, 24);
            (0..width).find(|x| buffer[(*x, 1)].symbol() == "┬")
        };
        assert_eq!(divider(80), Some(36), "the counted divider moved");
        // 160 columns: 158 inside the box, less the divider, split 35 : 42 — so the
        // list gets 71 and the divider lands at 72.
        assert_eq!(divider(160), Some(72));
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

    /// Renders the Mines sub-tab with `detail` swapped into the fixture.
    ///
    /// The pane's job is turning a *typed* detail into lines, so the test hands it the
    /// type rather than a run that would happen to project to it: reaching a maxed
    /// richness ceiling or a locked mine by play is `view`'s problem, and it is tested
    /// there. What is tested here is the sentence each shape prints.
    fn mine_pane_frame(detail: MineTrackDetail) -> String {
        let mut view = View::sample();
        view.upgrades.active = UpgradeTab::Mines;
        view.upgrades.mines.detail = UpgradeDetail::Mine(detail);
        whole_frame(&render_view(&view))
    }

    /// The Obsidian richness track the §5.4.2 frame draws, as a starting point to vary.
    fn a_mine_track() -> MineTrackDetail {
        MineTrackDetail {
            kind: MineKind::Obsidian,
            track: MineTrack::Richness,
            level: (6, 7),
            at_next: TrackOutcome::Richness(73),
            cost: Some(vec![CostLine {
                material: Material::Obsidian,
                compressed: 2,
                raw: 0,
            }]),
            held: vec![(Material::Obsidian, 0, 21)],
            mark: Mark::Refused,
            blocked: None,
        }
    }

    #[test]
    fn a_size_track_prints_the_grid_the_next_level_would_grow_to() {
        // Cells, not a percentage: the size track's whole product is a bigger grid, and
        // the number the player is buying is the one the Mine screen will draw.
        let frame = mine_pane_frame(MineTrackDetail {
            track: MineTrack::Size,
            at_next: TrackOutcome::Size((12, 7)),
            ..a_mine_track()
        });
        assert!(frame.contains("Obsidian Mine — size"), "{frame}");
        assert!(frame.contains("Size      level 6 → 7"), "{frame}");
        assert!(frame.contains("At 7      12x7 cells"), "{frame}");
        // The four lines about the dial belong to the richness track alone.
        assert!(!frame.contains("This buys the ceiling only"), "{frame}");
    }

    #[test]
    fn a_maxed_track_quotes_no_price_and_promises_no_next_level() {
        // `—` in the level line and "nothing left to buy" where a price would be: two
        // readings of one fact, because the pane's two halves are read separately.
        let frame = mine_pane_frame(MineTrackDetail {
            at_next: TrackOutcome::Maxed,
            cost: None,
            held: Vec::new(),
            mark: Mark::NoPrice,
            ..a_mine_track()
        });
        assert!(
            frame.contains(&format!("Ceiling   level 6 — {MAXED}")),
            "{frame}"
        );
        assert!(frame.contains("nothing left to buy"), "{frame}");
        assert!(!frame.contains("At 7"), "{frame}");
    }

    #[test]
    fn a_locked_mine_says_what_it_is_waiting_for_rather_than_what_it_costs() {
        // Both axes at once, which is what the End is on a fresh run: a level *and* a
        // tier, printed as one sentence over two lines rather than as two refusals.
        let state = GameState::new(1, UNIX_EPOCH);
        let lock = state.player().mine_lock(MineKind::Amethyst);
        let frame = mine_pane_frame(MineTrackDetail {
            kind: MineKind::Amethyst,
            blocked: Some(TrackBlock::Locked(lock)),
            ..a_mine_track()
        });
        assert!(frame.contains("Locked"), "{frame}");
        assert!(frame.contains("needs level 30"), "{frame}");
        assert!(frame.contains("and a Netherite pickaxe"), "{frame}");
        // A locked track prints no price at all — there is nothing to weigh yet.
        assert!(!frame.contains("You hold"), "{frame}");
    }

    #[test]
    fn an_unopened_mine_is_sent_to_the_mines_screen_rather_than_priced() {
        // The one refusal on this screen the player fixes by *going somewhere*, so the
        // pane names the tab and the key instead of a shortfall they do not have.
        let frame = mine_pane_frame(MineTrackDetail {
            kind: MineKind::Coal,
            blocked: Some(TrackBlock::NotEntered),
            ..a_mine_track()
        });
        assert!(frame.contains("Not yet"), "{frame}");
        assert!(frame.contains("2 Mines"), "{frame}");
        assert!(!frame.contains("Cost"), "{frame}");
    }

    #[test]
    fn a_capped_enchant_pane_says_there_is_nothing_left_to_buy() {
        let mut view = View::sample();
        view.upgrades.active = UpgradeTab::Enchants;
        view.upgrades.enchants.detail = UpgradeDetail::Enchant(EnchantDetail {
            kind: EnchantType::Explosive,
            level: 6,
            cap: 6,
            effect: vec!["clears a 5x5 square on a proc".to_owned()],
            next: vec!["at its cap — nothing left to buy".to_owned()],
            cost: None,
            mark: Mark::NoPrice,
        });
        let frame = whole_frame(&render_view(&view));
        assert!(frame.contains("nothing left to buy"), "{frame}");
    }

    #[test]
    fn a_chain_of_one_is_a_rung_and_not_rungs() {
        // A plural that is wrong on the single commonest purchase in the game — one
        // Efficiency level — is worth the branch it costs.
        let mut view = View::sample();
        let detail = match &view.upgrades.pickaxe.detail {
            UpgradeDetail::Pickaxe(detail) => PickaxeDetail {
                chain: vec!["Diamond Eff V".to_owned()],
                ..(**detail).clone()
            },
            other => unreachable!("the fixture's Pickaxe pane is a pickaxe: {other:?}"),
        };
        view.upgrades.pickaxe.detail = UpgradeDetail::Pickaxe(Box::new(detail));
        let frame = whole_frame(&render_view(&view));
        assert!(frame.contains("Chain     1 rung"), "{frame}");
        assert!(!frame.contains("1 rungs"), "{frame}");
    }
}
