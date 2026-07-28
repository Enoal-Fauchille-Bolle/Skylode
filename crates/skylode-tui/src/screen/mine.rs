//! The Mine screen — active mining (UI.md §5.1).
//!
//! The **grid** is fixed, not the layout, and that distinction is the screen's whole
//! claim: a **Haul** strip of three rows on top, then a middle band holding the grid
//! in a `Length(42)` panel beside a right column of two panels (Pickaxe, Mine), then
//! three `LineGauge` status rows, and a footer.
//!
//! The panel stays 42 columns wide at every terminal size because 42 is *already* the
//! largest mine — 20 cells × 2 columns plus two borders — so widening it could only
//! add margin. Its height is the opposite case: the band grows, and `MineGrid`
//! centres the mine inside whatever it is given, so the reserved area breathes while
//! the mine itself never changes size. Mine size is a game constant per mine, and the
//! window must not be able to change balance (§1).
//!
//! `LineGauge` and not `Gauge` for the status rows: a `Gauge` needs its own
//! `Block` and costs three rows, so three of them would eat nine rows the 24-row
//! budget does not have. A `LineGauge` is one row, label included.

use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{LineGauge, Paragraph},
};
use skylode_core::tunables::RAW_PER_COMPRESSED;

use crate::{action::Action, format::grouped, screen::panel, theme, view::View, widget::MineGrid};

/// The grid panel's fixed width — 40 columns of content plus its two borders. A
/// 12-wide mine is 24 columns and centres inside it; the largest mine still fits.
const GRID_PANEL_WIDTH: u16 = 42;

/// Width every status-gauge label is padded to, so the three bars start on the
/// same column however long each label is (the longest, XP, is 26).
const GAUGE_LABEL_WIDTH: usize = 28;

/// Blank rows kept between the gauges and the footer.
///
/// Frozen rather than flexed, and it is what pins the counted frame: with the four
/// fixed bands taking 11 of the 23 body rows, the middle band is left with the 12
/// UI-EN.md §4.2 counted. It is also where a toast lands, which is why the gap is
/// four rows and not zero — a three-row toast needs somewhere to sit that is not on
/// top of a gauge.
const SPACER_ROWS: u16 = 4;

/// Draws the whole screen into `area`.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    // Top to bottom: the strip, the panels, the gauges, a spacer, then the footer.
    // **The middle band is the one that flexes now**, and the spacer is frozen at the
    // four rows it happened to have at 80×24.
    //
    // The two used to be the other way round, and that was wrong for a reason worth
    // writing down: the Pickaxe and Mine panels live *inside* the middle band, so
    // freezing the band at twelve rows froze them too — a taller terminal grew a
    // hole between the gauges and the footer and gave the panels beside the grid not
    // one row. The grid itself is still a game constant (`MineGrid` centres it in
    // whatever it is handed), so what grows is the reserve around it, never the mine.
    //
    // At 23 body rows the four `Length`s take 11 and the `Fill` is left with exactly
    // 12 — the counted band, by subtraction rather than by coincidence.
    let [haul_area, middle_area, gauges_area, _spacer, footer_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Length(SPACER_ROWS),
        Constraint::Length(1),
    ])
    .areas(area);

    haul(frame, haul_area, view);
    middle(frame, middle_area, view);
    gauges(frame, gauges_area, view);
    footer(frame, footer_area);
}

/// The Haul strip: a fixed three-row box, one content line, on every mine.
fn haul(frame: &mut Frame, area: Rect, view: &View) {
    let material = view.mine_kind.common_material().name();
    // Value is the raw total across both denominations, in the denomination the
    // strip quotes it in — the same arithmetic `Inventory::raw_value` performs,
    // done here because it is a display sum and not a rule the core must own.
    let value = view.raw_held + view.compressed_held * RAW_PER_COMPRESSED;

    let block = panel(&format!(" Haul · {} ", view.mine_name));
    let line = format!(
        "  {material}   {} Raw   {} Compressed        value {} {material}",
        grouped(view.raw_held),
        grouped(view.compressed_held),
        grouped(value),
    );
    frame.render_widget(Paragraph::new(line).block(block), area);
}

/// The middle band: the grid panel at a fixed width, the right column beside it.
fn middle(frame: &mut Frame, area: Rect, view: &View) {
    let [grid_area, right_area] =
        Layout::horizontal([Constraint::Length(GRID_PANEL_WIDTH), Constraint::Min(0)]).areas(area);

    grid_panel(frame, grid_area, view);

    // The right column splits into the Pickaxe panel over the Mine panel, evenly.
    // At the counted 12 rows that is 6 and 6 — the same split `Length(6) + Min(0)`
    // produced — and above it the two grow together rather than the lower one
    // absorbing every spare row and towering over the one above it.
    let [pickaxe_area, mine_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Fill(1)]).areas(right_area);
    pickaxe_panel(frame, pickaxe_area, view);
    mine_panel(frame, mine_area, view);
}

/// The grid, centred in a panel titled with the mine's name.
fn grid_panel(frame: &mut Frame, area: Rect, view: &View) {
    let block = panel(&format!(" {} ", view.mine_name));

    // `inner` before `render_widget`, because rendering consumes the block: the
    // grid has to be told the area *inside* the border or it would paint over it.
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut grid = MineGrid::new(view.mine_kind, &view.grid).mode(view.colour_mode);
    // Chained conditionally rather than passed as an `Option`: the widget models
    // "nothing is being dug" as the absence of the call, so that a break ratio
    // cannot exist without a cell for it to be about.
    if let Some(cell) = view.target {
        grid = grid.target(cell, view.break_ratio);
    }
    frame.render_widget(grid, inner);
}

/// The Pickaxe panel: derived numbers only, not the full enchant roster (§5.1).
fn pickaxe_panel(frame: &mut Frame, area: Rect, view: &View) {
    let pickaxe = &view.pickaxe;
    // The product is shown, not just the two factors, because `power × boost` is
    // the number the player actually mines at — the factors are the explanation.
    let product = pickaxe.power * view.boost.multiplier;
    let lines = vec![
        Line::from(format!(" {}", pickaxe.summary)),
        Line::from(format!(
            " Power  {:.1}   ×{:.1} boost → {:.1}",
            pickaxe.power, view.boost.multiplier, product,
        )),
        Line::from(format!(" {}", pickaxe.fortune)),
        Line::from(format!(" Ench   {}", pickaxe.enchants)),
    ];
    frame.render_widget(Paragraph::new(lines).block(panel(" Pickaxe ")), area);
}

/// The Mine panel: the standing mine's own figures, most counted from the grid.
fn mine_panel(frame: &mut Frame, area: Rect, view: &View) {
    let panel_view = &view.mine_panel;

    // Rectangular by construction, so the first row's width is the grid's, and a
    // hole is a `None` — the block count is a walk of the standing cells rather
    // than a number the core has to keep in sync with the grid it already owns.
    let columns = view.grid.first().map_or(0, Vec::len);
    let rows = view.grid.len();
    let total = columns * rows;
    let standing = view.grid.iter().flatten().filter(|c| c.is_some()).count();

    let world = view.mine_kind.world().name();
    let lines = vec![
        Line::from(format!(" {:<22}{world}", view.mine_name)),
        Line::from(format!(
            " Blocks    {} / {}",
            grouped(standing as u32),
            grouped(total as u32),
        )),
        Line::from(format!(
            " Size      {columns} x {rows}   (level {})",
            panel_view.size_level,
        )),
        Line::from(format!(
            " Richness  level {} / {}   value {}%",
            panel_view.richness_level, panel_view.richness_max, panel_view.value_percent,
        )),
    ];
    frame.render_widget(Paragraph::new(lines).block(panel(" Mine ")), area);
}

/// The three status gauges, stacked one row each.
fn gauges(frame: &mut Frame, area: Rect, view: &View) {
    let [break_area, xp_area, boost_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let percent = (view.break_ratio * 100.0).round() as u32;
    gauge(
        frame,
        break_area,
        &format!(" Break  {percent}%  {}", view.target_name),
        ratio(f64::from(view.break_ratio)),
    );
    gauge(
        frame,
        xp_area,
        &format!(
            " XP  Lv {}   {} / {}",
            view.player_level,
            grouped(view.xp),
            grouped(view.xp_to_next),
        ),
        ratio(f64::from(view.xp) / f64::from(view.xp_to_next)),
    );
    gauge(
        frame,
        boost_area,
        &format!(
            " Boost  {}s  ×{:.2}",
            view.boost.seconds, view.boost.multiplier
        ),
        ratio(f64::from(view.boost.ratio)),
    );
}

/// Draws one `LineGauge` with a fixed-width label so its bar aligns with the rest.
///
/// The filled half takes [`theme::ACCENT`] and the empty half [`theme::MUTED`],
/// which is the same pair `screen::scrollbar` uses: both widgets answer "how far
/// along are you", and answering it in two colours would invent a distinction
/// neither of them makes.
fn gauge(frame: &mut Frame, area: Rect, label: &str, ratio: f64) {
    let gauge = LineGauge::default()
        .label(format!("{label:<GAUGE_LABEL_WIDTH$}"))
        .ratio(ratio)
        .filled_symbol("█")
        .unfilled_symbol("░")
        .filled_style(Style::default().fg(theme::ACCENT))
        .unfilled_style(Style::default().fg(theme::MUTED));
    frame.render_widget(gauge, area);
}

/// The footer line — the screen-local bindings; `q`/`s` stay off it by design.
///
/// Muted, like every other footer: a key hint is the least urgent thing on the
/// screen, and the whole point of giving the chrome a de-emphasised colour is that
/// the rows above it can then be read without competition.
fn footer(frame: &mut Frame, area: Rect) {
    let line = " Space  mine     Tab  next screen     ?  help";
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(theme::MUTED)),
        area,
    );
}

/// A ratio clamped into the unit range that `LineGauge::ratio` demands.
///
/// `LineGauge::ratio` **panics** outside `0.0..=1.0`, and this crate forbids a
/// panic in production, so every ratio passes through here first. A non-finite
/// value — an XP gauge dividing by a zero `xp_to_next`, say — maps to `0.0` rather
/// than propagating a `NaN` that `clamp` would pass straight through to the panic.
fn ratio(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// No contextual bindings yet; `Space` (mine) arrives with the tick in phase 7.
pub fn map_key(_key: KeyEvent) -> Option<Action> {
    None
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Modifier};

    use super::*;

    /// Renders the Mine screen alone into an 80×24 buffer.
    ///
    /// The screen is given the whole area — there is no tab bar in this harness —
    /// so its Haul strip lands at row 0, which is what the row arithmetic below
    /// counts from. Infallible `TestBackend` errors are discharged with an empty
    /// `match`, the same way `app.rs` does, rather than an `unwrap` the lints flag.
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

    /// The symbol at `(x, y)`.
    fn sym(buffer: &Buffer, x: u16, y: u16) -> &str {
        buffer[(x, y)].symbol()
    }

    /// Every row joined into one string — for "is this text on screen anywhere".
    /// The `(top, bottom)` rows of the grid panel's own box, found by its corners.
    fn grid_box_rows(buffer: &Buffer) -> (u16, u16) {
        // The Haul strip's `╭` is at row 0 too, so the grid panel's is the *second*
        // one down column 0 — and its `╰` is the second from the top likewise.
        let down_the_left = |glyph: &'static str| {
            (0..buffer.area.height)
                .filter(|y| sym(buffer, 0, *y) == glyph)
                .nth(1)
                .unwrap_or(0)
        };
        (down_the_left("╭"), down_the_left("╰"))
    }

    /// How many rows carry a painted swatch — the mine's own height on screen.
    fn grid_rows_painted(buffer: &Buffer) -> usize {
        (0..buffer.area.height)
            .filter(|y| {
                (0..buffer.area.width).any(|x| buffer[(x, *y)].bg != ratatui::style::Color::Reset)
            })
            .count()
    }

    #[test]
    fn the_counted_band_is_twelve_rows_at_the_reference_size() {
        // **23 rows, not 24**: that is what the app hands this screen once the tab
        // bar has taken its own, and the twelve-row band UI-EN.md §4.2 counted is a
        // fact about *that* area. The four fixed bands take 11 of the 23 and `Fill`
        // is left with 12 — rows 3..=14, the Haul strip occupying 0..=2 above it.
        let (top, bottom) = grid_box_rows(&render_sized(80, 23));
        assert_eq!((top, bottom), (3, 14), "the counted middle band moved");
    }

    #[test]
    fn a_taller_terminal_grows_the_panel_but_never_the_mine() {
        // Enoal's rule, as two assertions: the reserved area breathes, the mine does
        // not. Mine size is a game constant per mine (UI-EN.md §4.1), so a window
        // that could change it could change balance.
        let counted = render_screen();
        let tall = render_sized(80, 48);

        let (top, bottom) = grid_box_rows(&tall);
        let (counted_top, counted_bottom) = grid_box_rows(&counted);
        assert!(
            bottom - top > counted_bottom - counted_top,
            "the panel stayed {} rows tall on a 48-row terminal",
            bottom - top
        );
        // The mine itself is the same ten rows of swatches either way.
        assert_eq!(grid_rows_painted(&tall), grid_rows_painted(&counted));
        assert_eq!(grid_rows_painted(&counted), 10);
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

    #[test]
    fn the_haul_strip_names_the_mine_and_sums_its_value() {
        let frame = whole_frame(&render_screen());
        assert!(frame.contains("Haul · Iron Mine"), "{frame}");
        // 480 raw + 2 compressed × 100 = 680, quoted in the common denomination.
        assert!(frame.contains("480 Raw"), "{frame}");
        assert!(frame.contains("2 Compressed"), "{frame}");
        assert!(frame.contains("value 680 Iron"), "{frame}");
    }

    #[test]
    fn the_pickaxe_panel_shows_the_boosted_power_product() {
        let frame = whole_frame(&render_screen());
        assert!(frame.contains("Diamond Pickaxe  Efficiency IV"), "{frame}");
        // 25.0 × 1.5 = 37.5, computed in the screen from the two factors.
        assert!(frame.contains("Power  25.0"), "{frame}");
        assert!(frame.contains("→ 37.5"), "{frame}");
        assert!(frame.contains("Fortune III"), "{frame}");
        assert!(frame.contains("Exp II"), "{frame}");
    }

    #[test]
    fn the_mine_panel_counts_blocks_from_the_grid() {
        // The counts are **recomputed from the fixture** rather than written down,
        // because the panel's whole claim is that it derives them: hardcoding
        // `76 / 84` tested the fixture, and broke the day the fixture changed size
        // without the derivation being wrong for a moment. Now swapping grid
        // fixtures leaves this test true, and breaking the derivation fails it.
        let view = View::sample();
        let rows = view.grid.len();
        let columns = view.grid.first().map_or(0, Vec::len);
        let standing = view.grid.iter().flatten().filter(|c| c.is_some()).count();

        let frame = whole_frame(&render_screen());
        assert!(
            frame.contains(&format!("Blocks    {standing} / {}", rows * columns)),
            "{frame}"
        );
        assert!(
            frame.contains(&format!("Size      {columns} x {rows}")),
            "{frame}"
        );
        assert!(frame.contains("(level 9)"), "{frame}");
        assert!(frame.contains("Richness  level 0 / 9"), "{frame}");
        assert!(frame.contains("value 10%"), "{frame}");
        assert!(frame.contains("Overworld"), "{frame}");
    }

    #[test]
    fn the_three_status_gauges_are_drawn_with_bars() {
        let frame = whole_frame(&render_screen());
        assert!(frame.contains("Break  61%  Iron Block"), "{frame}");
        assert!(frame.contains("XP  Lv 23   1 240 / 2 300"), "{frame}");
        assert!(frame.contains("Boost  12s  ×1.50"), "{frame}");
        // The bar itself: a filled and an unfilled cell must both be painted, or
        // the gauge collapsed to text.
        assert!(frame.contains('█'), "no filled bar: {frame}");
        assert!(frame.contains('░'), "no unfilled bar: {frame}");
    }

    #[test]
    fn the_grid_panel_is_forty_two_columns_wide() {
        // The load-bearing invariant of the whole screen (§1). The middle band's
        // top border is row 3, just under the three-row Haul strip: the grid panel
        // closes at column 41 and the right column opens at column 42.
        let buffer = render_screen();
        assert_eq!(
            sym(&buffer, 0, 3),
            "╭",
            "grid panel does not start at col 0"
        );
        assert_eq!(sym(&buffer, 41, 3), "╮", "grid panel is not 42 wide");
        assert_eq!(sym(&buffer, 42, 3), "╭", "right column does not open at 42");
    }

    #[test]
    fn the_footer_advertises_the_screen_local_keys_only() {
        let buffer = render_screen();
        // Pinned to the last row, and it names `?` (global) and `Space` (local)
        // but never `q` or `s`, which the design keeps off every footer.
        let last = (0..buffer.area.width)
            .map(|x| sym(&buffer, x, 23))
            .collect::<String>();
        assert!(last.contains("Space  mine"), "{last:?}");
        assert!(last.contains("?  help"), "{last:?}");
        assert!(
            !last.contains(" q "),
            "a global key leaked into the footer: {last:?}"
        );
    }

    #[test]
    fn a_ratio_out_of_range_or_non_finite_never_reaches_the_gauge_panic() {
        // `ratio` exists only to keep `LineGauge::ratio` — which panics outside
        // `0.0..=1.0` — from ever seeing an illegal value, so its edges are the
        // whole point: an in-range value passes, an over/under value clamps, and
        // a non-finite one (a zero `xp_to_next` divides to infinity) maps to 0.
        assert_eq!(ratio(0.61), 0.61);
        assert_eq!(ratio(1.5), 1.0);
        assert_eq!(ratio(-0.2), 0.0);
        assert_eq!(ratio(f64::INFINITY), 0.0);
        assert_eq!(ratio(f64::NAN), 0.0);
    }

    #[test]
    fn all_three_gauges_are_drawn_in_the_accent_and_muted_pair() {
        // Scanned **only over the gauge band**, because `░` is also the value cell's
        // stipple in the grid above: a whole-buffer search would find a swatch's ink
        // and assert the wrong thing about the wrong widget.
        //
        // The band's rows are *derived*, not written down. Three of them sit directly
        // under the grid panel's bottom border, which `grid_box_rows` already locates
        // — and it locates it by looking, so this stays right when the middle band
        // flexes. A literal (`15..18` was one) is a number that agrees with the layout
        // only until the next constraint changes, and it fails silently in the
        // direction that matters: a band shifted down still finds a filled `█` on the
        // border row and never reaches the last gauge at all.
        let buffer = render_screen();
        let (_, grid_bottom) = grid_box_rows(&buffer);

        // Each gauge asserted on its own row, so all three are covered rather than
        // whichever one the scan reached first. `Break`, `XP`, `Boost`, in order.
        for (offset, name) in [(1, "Break"), (2, "XP"), (3, "Boost")] {
            let y = grid_bottom + offset;
            let first = |glyph: &str| {
                (0..buffer.area.width)
                    .map(|x| &buffer[(x, y)])
                    .find(|cell| cell.symbol() == glyph)
                    .map(|cell| cell.fg)
            };
            assert_eq!(first("█"), Some(theme::ACCENT), "{name}'s filled half");
            assert_eq!(first("░"), Some(theme::MUTED), "{name}'s unfilled half");
        }
    }

    #[test]
    fn a_panel_title_is_bold_as_well_as_coloured() {
        // The Haul strip's title sits on row 0. Both channels again: a terminal
        // that drops the hue must still show a title as a title.
        let buffer = render_screen();
        let title = (0..buffer.area.width)
            .map(|x| &buffer[(x, 0)])
            .find(|cell| cell.symbol() == "H");
        // Asserted through the `Option` rather than unwrapped: a missing title
        // fails on the `None` instead of panicking, which this crate's lints ask
        // for even in tests.
        assert_eq!(title.map(|cell| cell.fg), Some(theme::ACCENT));
        assert!(title.is_some_and(|cell| cell.modifier.contains(Modifier::BOLD)));
    }
}
