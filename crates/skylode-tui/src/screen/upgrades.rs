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
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use skylode_core::world::World;

use crate::{
    action::Action,
    cursor::{MineTrack, UpgradeTab},
    format::{MAXED, grouped, justified, roman},
    screen::{panel, scrollbar, window},
    theme,
    view::{
        DipDetail, EnchantDetail, Mark, MineTrackDetail, NOTHING, OwnedRung, PickaxeDetail,
        PowerDetail, PriceLine, StatStep, TrackBlock, TrackOutcome, UpgradeDetail, UpgradeSubtab,
        UpgradesView, View, level_word,
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
///
/// It takes the `Rect`'s height, which all three panes read: a price is the one block on
/// this screen with no bound — a chain from Wooden to Netherite Eff XV is forty-five
/// rungs, and an enchant level is priced in three materials — so it is cut to whatever
/// the blocks around it leave. See [`assembled`].
fn detail(frame: &mut Frame, area: Rect, subtab: &UpgradeSubtab) {
    let text = match &subtab.detail {
        UpgradeDetail::Pickaxe(detail) => pickaxe_pane(detail, usize::from(area.height)),
        UpgradeDetail::Enchant(detail) => enchant_pane(detail, usize::from(area.height)),
        UpgradeDetail::Mine(detail) => mine_pane(detail, usize::from(area.height)),
    };
    // Through `marked_row`: the pane quotes the affordability of what is selected, so
    // the same `✓ ~ ✗` appear here as in the list beside it, in the same hues — and
    // each line hands over the two things a glyph scan cannot derive, where its label
    // ends and what the rest is tinted with.
    let lines: Vec<Line> = text
        .iter()
        .map(|line| theme::marked_row(&line.text, line.label, line.tint))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// One line of a detail pane: the finished text, how many leading columns are the
/// block's label, and the hue the rest of it takes.
///
/// **Two numbers beside a string, and not a `Line` of spans.** Every pane below
/// composes its rows with [`format!`] and [`justified`], both of which measure the
/// *whole* row — so a pane that emitted spans would have to do its padding before it
/// knew its own width. Carrying the two facts the glyph scan cannot recover leaves all
/// of that arithmetic exactly where it was and moves nothing into `theme`.
struct PaneLine {
    /// The row as it will be drawn, already formatted and already justified.
    text: String,
    /// How many leading columns [`block`] spent on its label, muted by
    /// [`theme::marked_row`]. Zero on a continuation line and on prose.
    label: usize,
    /// What the rest of the row is tinted with — [`theme::of_glyph`]'s answer for the
    /// line's own mark, or [`None`] for a row that states no verdict.
    tint: Option<Color>,
}

impl From<String> for PaneLine {
    /// Prose: no label to mute, no verdict to tint by.
    ///
    /// Here so the panes keep pushing plain `format!` strings — the alternative is a
    /// constructor call on every one of the forty rows this screen builds, which would
    /// bury the three that actually carry a colour.
    fn from(text: String) -> Self {
        Self {
            text,
            label: 0,
            tint: None,
        }
    }
}

/// The columns [`block`] spends before a value: one of margin, nine of label, one of
/// gap.
///
/// Muted as a whole rather than just the label's own characters. The two spaces have
/// no ink to recolour, so the wider span is the same picture and one number instead of
/// three.
const LABEL_COLUMNS: usize = 11;

/// The columns a value has left once [`LABEL_COLUMNS`] is spent, in the counted frame.
///
/// Not enforced anywhere — a longer value is clipped by ratatui, not wrapped — but it is
/// the number every sentence in this module was measured against, and the one to check a
/// new one with.
const VALUE_COLUMNS: usize = DETAIL_WIDTH - LABEL_COLUMNS;

/// A labelled block: the label once, then one line per value, the rest indented under
/// it — the shape every pane in §5.4 repeats.
fn block(label: &str, values: &[String]) -> Vec<PaneLine> {
    let mut lines = Vec::new();
    for (index, value) in values.iter().enumerate() {
        if index == 0 {
            lines.push(PaneLine {
                text: format!(" {label:<9} {value}"),
                label: LABEL_COLUMNS,
                tint: None,
            });
        } else {
            lines.push(format!("           {value}").into());
        }
    }
    lines
}

/// A price, one row per material and denomination, each verdicted on its own.
///
/// **The block used to take one colour for the whole price**, from the verdict on the
/// whole [`Cost`](skylode_core::economy::Cost) — so a two-material price short of a
/// single ore was painted red end to end and said nothing about which half was missing.
/// The mark and the hue now sit on the line they are about, and a line that is not
/// [`Mark::Affordable`] is followed by what it is short of.
///
/// §4.5's rule survives intact, and is in fact better served: the hue is
/// [`theme::of_glyph`]'s answer for the very glyph justified onto that row, so it still
/// doubles a mark that is on screen — one per row now, rather than one for the block.
///
/// The shortfall row is deliberately **untinted**: it is the explanation of a mark, not
/// a second statement of it, and two red lines in a row read as two refusals.
fn price_block(lines: &[PriceLine]) -> Vec<PaneLine> {
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let value = format!("{} {}", grouped(line.needed), line.item);
        let row = if index == 0 {
            format!(" {:<9} {value}", "Cost")
        } else {
            format!("           {value}")
        };
        out.push(PaneLine {
            text: justified(&row, &format!("{} ", line.mark.glyph()), DETAIL_WIDTH),
            label: if index == 0 { LABEL_COLUMNS } else { 0 },
            tint: theme::of_glyph(line.mark.glyph()),
        });
        if line.mark != Mark::Affordable {
            out.push(
                format!(
                    "             hold {} — short {}",
                    grouped(line.held),
                    grouped(line.needed.saturating_sub(line.held))
                )
                .into(),
            );
        }
    }
    out
}

/// What an upgrade moves, one row per stat: the stat's word, then its `now → next`.
///
/// The name column is measured over the rows it is given rather than fixed, for the
/// reason [`list`] measures its own: `square` and `row` differ by three columns, and a
/// constant wide enough for both would push every Haste row off-centre for nothing.
fn stat_block(label: &str, steps: &[StatStep]) -> Vec<PaneLine> {
    let width = steps.iter().map(|step| step.name.len()).max().unwrap_or(0);
    let values: Vec<String> = steps
        .iter()
        .map(|step| format!("{:<width$}  {}", step.name, step.value))
        .collect();
    block(label, &values)
}

/// The Pickaxe pane (UI.md §5.4).
///
/// **Built in three parts because one of them is unbounded.** The head (the title and
/// the chain) and the tail (the power, what the chain unlocks and the Efficiency
/// ceiling) are a handful of rows each; the price between them is however many
/// materials forty-five rungs of ladder demand. So the two fixed ends are measured
/// first and the price gets what is left — the block that overflows is the block that
/// is cut, rather than the `Ceiling` line that happened to be last.
fn pickaxe_pane(detail: &PickaxeDetail, height: usize) -> Vec<PaneLine> {
    let mut head = vec![
        justified(
            &format!(" {}", detail.title),
            if detail.crosses_tier_jump {
                "tier jump "
            } else {
                ""
            },
            DETAIL_WIDTH,
        )
        .into(),
        PaneLine::from(String::new()),
    ];

    if let Some(owned) = &detail.owned {
        head.push(" Owned already — nothing to buy here.".to_owned().into());
        head.push(String::new().into());
        head.extend(owned_block(owned));
        return head;
    }

    let rungs = if detail.chain.len() == 1 {
        "1 rung".to_owned()
    } else {
        format!("{} rungs", detail.chain.len())
    };
    // Hand-built rather than through `block`, because the mark is justified onto this
    // line rather than onto the price below it — but it is a labelled row all the
    // same, so it claims the same muted columns.
    head.push(PaneLine {
        text: justified(
            &format!(" {:<9} {rungs}", "Chain"),
            &format!("{} ", detail.mark.glyph()),
            DETAIL_WIDTH,
        ),
        label: LABEL_COLUMNS,
        tint: None,
    });

    let mut tail = vec![PaneLine::from(String::new())];
    match &detail.dip {
        Some(dip) => tail.extend(dip_box(&detail.power, dip)),
        None => tail.extend(power_block(&detail.power)),
    }
    if !detail.unlocks.is_empty() {
        tail.push(String::new().into());
        let names: Vec<String> = detail
            .unlocks
            .iter()
            .map(|kind| format!("the {} mine", kind.name()))
            .collect();
        tail.extend(block("Unlocks", &names));
    }
    if let Some((before, after)) = detail.ceiling {
        tail.push(String::new().into());
        tail.extend(block(
            "Ceiling",
            &[format!("Efficiency {before} → {after}")],
        ));
    }

    assembled(head, price_block(&detail.costs), tail, height)
}

/// A pane's three parts, with the price cut to whatever the other two leave.
///
/// **The price is the only block on this screen that grows without bound**: three
/// materials in two denominations each, and a shortfall row under every one that is
/// short. So the two fixed ends are measured first and the price gets the remainder —
/// the block that overflows is the block that is cut, rather than the `Ceiling` or `Cap`
/// line that happened to be last. Losing either of those is losing the number the
/// purchase is decided on; losing the fifth material of a price the player cannot afford
/// anyway is losing very little.
///
/// **A count of *lines*, not of rungs**, in the tail it leaves behind. The pickaxe chain
/// is aggregated per material by the time it reaches here, so the rows that were cut are
/// materials and denominations.
fn assembled(
    head: Vec<PaneLine>,
    price: Vec<PaneLine>,
    tail: Vec<PaneLine>,
    height: usize,
) -> Vec<PaneLine> {
    let budget = height.saturating_sub(head.len() + tail.len());
    let mut lines = head;
    if price.len() <= budget {
        lines.extend(price);
    } else {
        let shown = budget.saturating_sub(1);
        let dropped = price.len() - shown;
        lines.extend(price.into_iter().take(shown));
        lines.push(format!("           …+ {dropped} more lines").into());
    }
    lines.extend(tail);
    lines
}

/// What a rung the player already owns is worth: the same four rows the buyable
/// rungs get, each with one value instead of two.
///
/// **Deliberately the same labels in the same order as [`power_block`] and the two
/// blocks under it**, so scrolling from an owned rung to a buyable one moves the
/// numbers and not the layout. The arrow is what is missing, and its absence is the
/// whole message: there is nothing here to move to.
///
/// `Unlocks` is skipped when the rung opened nothing, which on this ladder means
/// every rung that is not a tier jump. `Ceiling` is never skipped — `Efficiency 0 / 5`
/// on a fresh tier is information, and it is the number that says how much of the tier
/// is left to buy.
///
/// Labelled `Ceiling` and not `Efficiency` for two reasons that agree: it is the word
/// the buyable pane already spends on this number, and [`block`] reserves nine columns
/// for a label — `Efficiency` is ten, and would push its own value out of the column
/// the two rows above it line up in.
fn owned_block(owned: &OwnedRung) -> Vec<PaneLine> {
    let mut lines = block("Power", &[format!("{:.1}", owned.power)]);
    lines.extend(block(
        "Ticks",
        &[format!("{} {}", owned.block.name(), ticks(owned.ticks))],
    ));
    let (level, cap) = owned.efficiency;
    lines.extend(block("Ceiling", &[format!("Efficiency {level} / {cap}")]));
    if !owned.unlocks.is_empty() {
        let names: Vec<String> = owned
            .unlocks
            .iter()
            .map(|kind| format!("the {} mine", kind.name()))
            .collect();
        lines.push(String::new().into());
        lines.extend(block("Unlocks", &names));
    }
    lines
}

/// What the chain does to the swing, on a rung that does not cost power.
///
/// **A labelled block and not the dip's box art**, which is the whole point of drawing
/// it here at all: the box is a *warning*, and one drawn on all forty-six rungs stops
/// being read as one. Its five rows of frame are also what the price block above it
/// needs back on a long chain.
///
/// The block is named on the `Ticks` row rather than used as a label, because
/// `Crying Obsidian` is fifteen columns against the nine [`block`] gives a label — the
/// name would push its own value out of the pane.
fn power_block(power: &PowerDetail) -> Vec<PaneLine> {
    let mut lines = block(
        "Power",
        &[format!("{:.1} → {:.1}", power.before, power.after)],
    );
    lines.extend(block(
        "Ticks",
        &[format!(
            "{} {} → {}",
            power.block.name(),
            ticks(power.ticks_before),
            ticks(power.ticks_after)
        )],
    ));
    lines
}

/// The dip box (UI.md §5.4): the same two numbers, framed as the warning they are.
///
/// Reads its powers from [`PowerDetail`] like every other rung does, so the box and the
/// plain block cannot quote the swing differently. What the dip adds is the repayment —
/// the rung that earns the loss back, which is the half of the decision the numbers
/// above it cannot state.
fn dip_box(power: &PowerDetail, dip: &DipDetail) -> Vec<PaneLine> {
    let mut lines: Vec<PaneLine> =
        vec![" ┌────────────────────────────────────┐".to_owned().into()];
    lines.push(boxed(&format!("Power  {:.1} → {:.1}", power.before, power.after)).into());
    lines.push(
        boxed(&format!(
            "{}  {} → {} ticks",
            power.block.name(),
            ticks(power.ticks_before),
            ticks(power.ticks_after)
        ))
        .into(),
    );
    if let Some(repaid) = &dip.repaid_at {
        lines.push(boxed(&format!("Repaid at {} ({:.1})", repaid.rung, repaid.power)).into());
    }
    lines.push(" └────────────────────────────────────┘".to_owned().into());
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
///
/// Assembled like the Pickaxe pane, for the same reason: a level of Explosive is priced
/// in three materials, and on a run that can afford none of them the price is twelve rows
/// against a nineteen-row pane. The block that must survive is `Cap` — the one thing on
/// this pane a player cannot work out from anywhere else.
fn enchant_pane(detail: &EnchantDetail, height: usize) -> Vec<PaneLine> {
    let mut head = vec![
        justified(
            &format!(" {}", detail.kind.name()),
            &format!("level {} ", level_word(detail.level)),
            DETAIL_WIDTH,
        )
        .into(),
        PaneLine::from(String::new()),
    ];
    head.extend(block("Effect", &detail.effect));
    head.push(String::new().into());

    if detail.at_next.is_empty() {
        head.extend(block("Next", &[format!("{MAXED} — nothing left to buy")]));
    } else {
        head.extend(stat_block(
            &format!("At {}", roman(detail.level + 1)),
            &detail.at_next,
        ));
    }
    for note in &detail.note {
        head.push(format!("           {note}").into());
    }
    head.push(String::new().into());

    let mut tail = vec![PaneLine::from(String::new())];
    tail.extend(block("Cap", &cap_sentence(detail.cap, detail.world)));

    let price = if detail.cost.is_empty() {
        block("Cost", &["nothing left to buy".to_owned()])
    } else {
        price_block(&detail.cost)
    };
    assembled(head, price, tail, height)
}

/// The `Cap` block's four lines (UI.md §5.4.1), which say three things the number alone
/// does not.
///
/// **`3` on its own is ambiguous in the way that matters**: a ceiling the player has hit
/// and one the *Overworld* is holding down call for opposite decisions — stop buying, or
/// go open the Nether. So the cap is quoted `3 of 10` against the game's own ceiling, the
/// world in force is named, and the two the player is not in are priced.
///
/// **Six tracks, not five.** The frame's own prose reads *"all five specials"* and
/// `docs/UI.md` §5.4.1 still adds *"while Fortune's 10 is its own"* — both predate
/// `DECISIONS.md`'s amendment, which put Fortune on [`World::enchant_cap`] with the other
/// five so that no lever in the game is maxable at level 1. `enchant.rs` implements the
/// amendment; this says what it implements.
fn cap_sentence(cap: u8, world: World) -> Vec<String> {
    // Written out because an enum cannot enumerate itself and the core has no `ALL` for
    // worlds — the same reason `MineKind::ALL` had to be added for the Mines screen.
    let others: Vec<String> = [World::Overworld, World::Nether, World::End]
        .into_iter()
        .filter(|&other| other != world)
        .map(|other| format!("{} {}", other.name(), other.enchant_cap()))
        .collect();
    wrapped(
        &format!(
            "{cap} of {} — the {}'s. Six tracks share it: {}. Efficiency is capped by the \
             pickaxe tier instead.",
            World::End.enchant_cap(),
            world.name(),
            others.join(", ")
        ),
        VALUE_COLUMNS,
    )
}

/// `text` broken into lines of at most `width` columns, on spaces.
///
/// **Here rather than as constant strings, because the sentence varies with the run.**
/// Every other prose block on this screen is fixed and was hand-broken to fit; the `Cap`
/// block names the world the player is in and the two they are not, so its line breaks
/// move as they progress and cannot be written down.
///
/// A word longer than `width` gets a line of its own and overruns it, which ratatui
/// clips. Nothing in the vocabulary here is close — `Efficiency` is the longest at ten.
fn wrapped(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(line) if line.chars().count() + 1 + word.chars().count() <= width => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_owned()),
        }
    }
    lines
}

/// The Mines pane (UI.md §5.4.2).
///
/// **Four lines of it are spent refusing one conflation**, and that is the frame's own
/// choice: richness is the only word in the game that appears next to a price *and*
/// next to a free cursor, and this is the one place both senses are on screen at once.
fn mine_pane(detail: &MineTrackDetail, height: usize) -> Vec<PaneLine> {
    let track = match detail.track {
        MineTrack::Size => "size",
        MineTrack::Richness => "richness",
    };
    let mut head = vec![
        PaneLine::from(format!(" {} Mine — {track}", detail.kind.name())),
        PaneLine::from(String::new()),
    ];

    if let Some(blocked) = detail.blocked {
        head.extend(match blocked {
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
        return head;
    }

    let (level, next) = detail.level;
    head.extend(block(
        match detail.track {
            MineTrack::Size => "Size",
            MineTrack::Richness => "Ceiling",
        },
        &[match detail.at_next {
            TrackOutcome::Maxed => format!("level {level} — {MAXED}"),
            _ => format!("level {level} → {next}"),
        }],
    ));

    // **Both sides of the step, which the frame's `At 7` block never showed.** A share or
    // a grid quoted only *after* leaves the player to remember what they are moving from,
    // and both numbers are free — the two tracks are pure functions of a level.
    match detail.at_next {
        TrackOutcome::Size { before, after } => {
            head.extend(stat_block(
                &format!("At {next}"),
                &[
                    StatStep {
                        name: "grid",
                        value: format!("{}x{} → {}x{}", before.0, before.1, after.0, after.1),
                    },
                    StatStep {
                        name: "cells",
                        value: format!(
                            "{} → {}",
                            grouped(u32::from(before.0) * u32::from(before.1)),
                            grouped(u32::from(after.0) * u32::from(after.1))
                        ),
                    },
                ],
            ));
        }
        TrackOutcome::Richness { before, after } => {
            head.extend(block("Dial", &["free, on the Mines screen".to_owned()]));
            head.push(String::new().into());
            head.extend(block(
                &format!("At {next}"),
                &[
                    format!("{} {before}% → {after}%", detail.kind.value_block().name()),
                    format!(
                        "{} {}% → {}%",
                        detail.kind.common_block().name(),
                        100 - before,
                        100 - after
                    ),
                ],
            ));
        }
        TrackOutcome::Maxed => {}
    }

    head.push(String::new().into());

    let mut tail = Vec::new();
    if detail.track == MineTrack::Richness {
        tail.push(String::new().into());
        tail.push(" This buys the ceiling only. The".to_owned().into());
        tail.push(" dial slides anywhere at or below".to_owned().into());
        tail.push(" it, free and reversible, on the".to_owned().into());
        tail.push(" Mines screen.".to_owned().into());
    }

    let price = if detail.cost.is_empty() {
        block("Cost", &["nothing left to buy".to_owned()])
    } else {
        price_block(&detail.cost)
    };
    assembled(head, price, tail, height)
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
        enchant::EnchantType,
        game::GameState,
        material::{Item, Material},
        mine_kind::MineKind,
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
        // And the row explaining the shortfall is not: it is the reading of a mark, not
        // a second statement of one, and two red rows in a row read as two refusals.
        assert_eq!(
            ink(&buffer, "short", PANE_X),
            Some(Color::Reset),
            "the tint leaked onto the shortfall it explains"
        );
    }

    /// **The defect this block was rewritten for.** A price short of one material used
    /// to be painted red end to end, so the player could see that it was refused and
    /// not which half refused it.
    #[test]
    fn each_material_of_a_price_is_verdicted_on_its_own() {
        let frame_buffer = mine_pane_buffer(MineTrackDetail {
            cost: vec![
                PriceLine {
                    item: Item::Compressed(Material::Obsidian),
                    needed: 2,
                    held: 7,
                    mark: Mark::Affordable,
                },
                PriceLine {
                    item: Item::Raw(Material::CryingObsidian),
                    needed: 40,
                    held: 2,
                    mark: Mark::Refused,
                },
            ],
            ..a_mine_track()
        });
        let frame = whole_frame(&frame_buffer);

        assert!(frame.contains("Cost      2 Compressed Obsidian"), "{frame}");
        assert!(frame.contains("40 Crying Obsidian"), "{frame}");
        // What the pane could not say before: the shortfall, on the line it is about.
        assert!(frame.contains("hold 2 — short 38"), "{frame}");
        // One green row and one red one, in a price the list column marks `✗`.
        assert_eq!(
            ink(
                &frame_buffer,
                "Compressed Obsidian",
                PANE_X + LABEL_COLUMNS as u16
            ),
            Some(theme::AFFORDABLE)
        );
        // Anchored on the amount as well as the material: `Crying Obsidian` is also the
        // value block named in the `At 7` rows above, and those are prose.
        assert_eq!(
            ink(&frame_buffer, "40 Crying Obsidian", PANE_X),
            Some(theme::REFUSED)
        );
        // And the affordable line gets no shortfall row under it.
        assert!(!frame.contains("hold 7"), "{frame}");
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

    /// The column the flush-right reachability marks sit in, or [`None`] on a
    /// sub-tab that draws none. Scanned over the list side's body rows only, so the
    /// detail pane's own `✓` cannot answer for the list's.
    fn mark_column(buffer: &Buffer) -> Option<u16> {
        (2..22).find_map(|y| (0..36).find(|&x| matches!(sym(buffer, x, y), "✓" | "~" | "✗")))
    }

    #[test]
    fn the_mark_column_keeps_one_blank_column_before_the_scrollbar() {
        // The §5.4 Pickaxe frame's gutter, at the width the arithmetic allows: the
        // mark in column 33, one blank column, then the bar in 35. A mark drawn
        // against the thumb reads as part of it.
        let buffer = render_tab(UpgradeTab::Pickaxe);
        let marked: Vec<u16> = (2..22)
            .filter(|&y| matches!(sym(&buffer, 33, y), "✓" | "~" | "✗"))
            .collect();
        assert!(
            !marked.is_empty(),
            "no mark in column 33:\n{}",
            whole_frame(&buffer)
        );
        for y in marked {
            assert_eq!(sym(&buffer, 34, y), " ", "the gutter is filled on row {y}");
            assert!(
                matches!(sym(&buffer, 35, y), "░" | "█"),
                "no bar beside row {y}"
            );
        }
    }

    #[test]
    fn the_mark_column_stays_put_on_a_sub_tab_that_does_not_scroll() {
        // Enchants fits in nineteen rows and draws no thumb; Pickaxe overflows and
        // draws one. The bar column is reserved on both, so the column of `✓ ~ ✗` is
        // the same either way — a glyph column that jumped when the player changed
        // sub-tab would read as a redraw fault rather than as a layout.
        let scrolling = mark_column(&render_tab(UpgradeTab::Pickaxe));
        let fitting = mark_column(&render_tab(UpgradeTab::Enchants));
        assert_eq!(scrolling, Some(33));
        assert_eq!(fitting, scrolling, "the marks moved with the scrollbar");
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

    /// An owned rung, as `from_state` projects one: no chain, no price, no dip.
    ///
    /// Varied off [`a_chain`] rather than built field by field, so a field added to
    /// [`PickaxeDetail`] reaches this shape too instead of quietly defaulting. The
    /// `power` it inherits is the fixture's `34.0 → 9.0`, which is exactly what the
    /// pane must **not** print here — `upgrade::preview` clamps a rung behind the
    /// player, so those two numbers describe the pickaxe and not the rung.
    fn an_owned_rung(owned: OwnedRung) -> PickaxeDetail {
        PickaxeDetail {
            title: "Iron Pickaxe".to_owned(),
            crosses_tier_jump: false,
            chain: Vec::new(),
            mark: Mark::Owned,
            costs: Vec::new(),
            dip: None,
            unlocks: Vec::new(),
            ceiling: None,
            owned: Some(owned),
            ..a_chain()
        }
    }

    #[test]
    fn an_owned_rung_states_what_it_is_worth_instead_of_stopping_at_the_sentence() {
        let frame = pickaxe_pane_frame(an_owned_rung(OwnedRung {
            power: 4.0,
            block: Block::IronBlock,
            ticks: Some(30),
            efficiency: (0, 5),
            unlocks: vec![MineKind::Iron],
        }));

        // The sentence stays: it is the answer to "can I buy this?", which is still a
        // question the player asked by putting the cursor here.
        assert!(frame.contains("Owned already"), "{frame}");
        // And under it, the numbers the pane used to withhold. Single-valued, so no
        // arrow — the absence is the message.
        assert!(frame.contains("Power     4.0"), "{frame}");
        assert!(!row_with(&frame, "Power     4.0").contains('→'), "{frame}");
        assert!(frame.contains("Ticks     Iron Block 30"), "{frame}");
        assert!(frame.contains("Ceiling   Efficiency 0 / 5"), "{frame}");
        assert!(frame.contains("Unlocks   the Iron mine"), "{frame}");
        // The clamped preview's numbers must not have leaked through beside them.
        assert!(!frame.contains("34.0"), "{frame}");
    }

    #[test]
    fn an_owned_rung_that_opened_nothing_prints_no_unlocks_row() {
        // An Efficiency rung is bought inside a tier the player already had, so it has
        // no mine to its name and the block is dropped rather than left empty.
        let frame = pickaxe_pane_frame(an_owned_rung(OwnedRung {
            power: 9.0,
            block: Block::IronBlock,
            ticks: Some(14),
            efficiency: (3, 5),
            unlocks: Vec::new(),
        }));
        assert!(frame.contains("Ceiling   Efficiency 3 / 5"), "{frame}");
        assert!(!frame.contains("Unlocks"), "{frame}");
    }

    /// Renders the Mines sub-tab with `detail` swapped into the fixture.
    ///
    /// The pane's job is turning a *typed* detail into lines, so the test hands it the
    /// type rather than a run that would happen to project to it: reaching a maxed
    /// richness ceiling or a locked mine by play is `view`'s problem, and it is tested
    /// there. What is tested here is the sentence each shape prints.
    fn mine_pane_frame(detail: MineTrackDetail) -> String {
        whole_frame(&mine_pane_buffer(detail))
    }

    /// The same, kept as a [`Buffer`] for the assertions that are about colour.
    fn mine_pane_buffer(detail: MineTrackDetail) -> Buffer {
        let mut view = View::sample();
        view.upgrades.active = UpgradeTab::Mines;
        view.upgrades.mines.detail = UpgradeDetail::Mine(detail);
        render_view(&view)
    }

    /// The Obsidian richness track the §5.4.2 frame draws, as a starting point to vary.
    fn a_mine_track() -> MineTrackDetail {
        MineTrackDetail {
            kind: MineKind::Obsidian,
            track: MineTrack::Richness,
            level: (6, 7),
            at_next: TrackOutcome::Richness {
                before: 66,
                after: 73,
            },
            cost: vec![PriceLine {
                item: Item::Compressed(Material::Obsidian),
                needed: 2,
                held: 0,
                mark: Mark::Refused,
            }],
            blocked: None,
        }
    }

    #[test]
    fn a_size_track_prints_the_grid_the_next_level_would_grow_to() {
        // Cells, not a percentage: the size track's whole product is a bigger grid, and
        // the number the player is buying is the one the Mine screen will draw.
        let frame = mine_pane_frame(MineTrackDetail {
            track: MineTrack::Size,
            at_next: TrackOutcome::Size {
                before: (10, 6),
                after: (12, 7),
            },
            ..a_mine_track()
        });
        assert!(frame.contains("Obsidian Mine — size"), "{frame}");
        assert!(frame.contains("Size      level 6 → 7"), "{frame}");
        // Both sides of the step, and the cell counts under them: a grid quoted only
        // after leaves the player to remember what they are moving from.
        assert!(frame.contains("At 7      grid   10x6 → 12x7"), "{frame}");
        assert!(frame.contains("cells  60 → 84"), "{frame}");
        // The four lines about the dial belong to the richness track alone.
        assert!(!frame.contains("This buys the ceiling only"), "{frame}");
    }

    #[test]
    fn a_maxed_track_quotes_no_price_and_promises_no_next_level() {
        // `—` in the level line and "nothing left to buy" where a price would be: two
        // readings of one fact, because the pane's two halves are read separately.
        let frame = mine_pane_frame(MineTrackDetail {
            at_next: TrackOutcome::Maxed,
            cost: Vec::new(),
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
        assert!(!frame.contains("Cost"), "{frame}");
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
            world: World::Nether,
            effect: vec!["clears a 5x5 square on a proc".to_owned()],
            at_next: Vec::new(),
            note: Vec::new(),
            cost: Vec::new(),
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

    /// A pickaxe pane with `detail` swapped in, rendered at 80×24.
    fn pickaxe_pane_frame(detail: PickaxeDetail) -> String {
        let mut view = View::sample();
        view.upgrades.active = UpgradeTab::Pickaxe;
        view.upgrades.pickaxe.detail = UpgradeDetail::Pickaxe(Box::new(detail));
        whole_frame(&render_view(&view))
    }

    /// The fixture's own pickaxe detail, as a starting point to vary.
    fn a_chain() -> PickaxeDetail {
        match &View::sample().upgrades.pickaxe.detail {
            UpgradeDetail::Pickaxe(detail) => (**detail).clone(),
            other => unreachable!("the fixture's Pickaxe pane is a pickaxe: {other:?}"),
        }
    }

    /// **The overflow this pane was rewritten for.** A chain from the bottom of the
    /// ladder to the top demands eight materials in two denominations each, every one of
    /// them short on a fresh run — sixteen price rows and sixteen shortfalls under them,
    /// in a pane nineteen rows tall.
    ///
    /// What must survive the cut is everything *below* the price: a `Ceiling` line
    /// pushed off the bottom is the one number a tier jump is decided on.
    #[test]
    fn a_chain_too_long_to_price_in_full_keeps_what_sits_under_it() {
        let long: Vec<PriceLine> = Material::ALL
            .into_iter()
            .flat_map(|material| {
                [
                    PriceLine {
                        item: Item::Compressed(material),
                        needed: 12,
                        held: 0,
                        mark: Mark::Refused,
                    },
                    PriceLine {
                        item: Item::Raw(material),
                        needed: 40,
                        held: 0,
                        mark: Mark::Refused,
                    },
                ]
            })
            .collect();
        let dropped = long.len();
        let frame = pickaxe_pane_frame(PickaxeDetail {
            costs: long,
            ..a_chain()
        });

        assert!(
            frame.contains("…+ "),
            "the price was not cut at all:\n{frame}"
        );
        assert!(frame.contains(" more lines"), "{frame}");
        // A count of *lines*, not of rungs: the price is aggregated per material by the
        // time it reaches the pane, so rungs are no longer what was dropped.
        assert!(!frame.contains("more rungs"), "{frame}");
        // Everything under the price is still on screen.
        assert!(frame.contains("Ceiling   Efficiency 5 → 15"), "{frame}");
        assert!(frame.contains("Unlocks"), "{frame}");
        assert!(frame.contains("Power  34.0 → 9.0"), "{frame}");
        // And it really was too long to fit, so the assertions above are about a cut.
        assert!(dropped > 20, "{dropped} rows is not an overflow");
    }

    /// The `Power` block is what an ordinary rung is bought for, and it used to be
    /// printed only when it went the *wrong* way.
    ///
    /// The box art stays off it: that frame is a warning, and one drawn on all
    /// forty-six rungs stops being read as one.
    #[test]
    fn a_rung_that_costs_no_power_still_says_what_it_buys() {
        let frame = pickaxe_pane_frame(PickaxeDetail {
            dip: None,
            power: PowerDetail {
                before: 34.0,
                after: 41.0,
                ..a_chain().power
            },
            ..a_chain()
        });

        assert!(frame.contains("Power     34.0 → 41.0"), "{frame}");
        assert!(
            frame.contains("Ticks     Ancient Debris 27 → 100"),
            "{frame}"
        );
        assert!(
            !frame.contains("┌────"),
            "the box art is the dip's:\n{frame}"
        );

        // And the dip still draws its own frame, with the repayment inside it.
        let dipped = pickaxe_pane_frame(a_chain());
        assert!(dipped.contains("┌────"), "{dipped}");
        assert!(dipped.contains("Repaid at Netherite Eff V"), "{dipped}");
        assert!(!dipped.contains("Power     34.0"), "{dipped}");
    }

    /// `Cap 3` alone cannot tell a ceiling the player has hit from one the *world* is
    /// holding down, and the two call for opposite decisions — stop buying, or go open
    /// the Nether.
    #[test]
    fn the_cap_names_the_world_that_sets_it_and_the_one_the_game_stops_at() {
        for (world, cap, others) in [
            (World::Overworld, 3, "Nether 6, End 10"),
            (World::Nether, 6, "Overworld 3, End 10"),
            (World::End, 10, "Overworld 3, Nether 6"),
        ] {
            let mut view = View::sample();
            view.upgrades.active = UpgradeTab::Enchants;
            let detail = match &view.upgrades.enchants.detail {
                UpgradeDetail::Enchant(detail) => EnchantDetail {
                    cap,
                    world,
                    ..detail.clone()
                },
                other => unreachable!("the fixture's Enchants pane is an enchant: {other:?}"),
            };
            view.upgrades.enchants.detail = UpgradeDetail::Enchant(detail);
            let frame = whole_frame(&render_view(&view));

            // On screen: the cap against the game's own ceiling, and whose it is.
            assert!(
                frame.contains(&format!("Cap       {cap} of 10 — the {}'s.", world.name())),
                "{frame}"
            );

            // The rest of the sentence is asserted unwrapped — its line breaks move with
            // the world, and the pane's right border sits between them in the frame.
            let sentence = cap_sentence(cap, world).join(" ");
            assert!(sentence.contains(others), "{sentence:?}");
            // Six tracks share it, not the five the frame's prose still says.
            assert!(sentence.contains("Six tracks share it"), "{sentence:?}");
            assert!(
                sentence.contains("Efficiency is capped by the pickaxe tier"),
                "{sentence:?}"
            );
        }
    }

    /// The `Cap` sentence is the one prose block on this screen whose line breaks move
    /// with the run, so it is wrapped rather than hand-broken. Nothing may overrun the
    /// columns a labelled block leaves.
    #[test]
    fn a_wrapped_sentence_never_overruns_the_value_column() {
        for world in [World::Overworld, World::Nether, World::End] {
            for line in cap_sentence(world.enchant_cap(), world) {
                assert!(
                    line.chars().count() <= VALUE_COLUMNS,
                    "{world:?} wraps to {} columns: {line:?}",
                    line.chars().count()
                );
            }
        }
        // Words are never split, and no line starts or ends on a space.
        let wrapped = wrapped("one two three four five six seven eight", 11);
        assert_eq!(
            wrapped,
            vec!["one two", "three four", "five six", "seven eight"]
        );
    }
}
