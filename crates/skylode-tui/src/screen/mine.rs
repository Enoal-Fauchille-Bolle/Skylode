//! The Mine screen — active mining (UI.md §5.1).
//!
//! The layout is fixed, not proportional, and that is the screen's whole claim:
//! a **Haul** strip of three rows on top, then a middle band holding the grid in a
//! `Length(42)` panel beside a right column of two panels (Pickaxe, Mine), then
//! three `LineGauge` status rows, and a footer. The grid never moves because the
//! chrome around it is measured in rows and columns, never in percentages (§1).
//!
//! `LineGauge` and not `Gauge` for the status rows: a `Gauge` needs its own
//! `Block` and costs three rows, so three of them would eat nine rows the 24-row
//! budget does not have. A `LineGauge` is one row, label included.

use ratatui::{
    Frame,
    crossterm::event::KeyEvent,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::Line,
    widgets::{Block, BorderType, Borders, LineGauge, Paragraph},
};
use skylode_core::tunables::RAW_PER_COMPRESSED;

use crate::{action::Action, format::grouped, view::View, widget::MineGrid};

/// The grid panel's fixed width — 40 columns of content plus its two borders. A
/// 12-wide mine is 24 columns and centres inside it; the largest mine still fits.
const GRID_PANEL_WIDTH: u16 = 42;

/// Width every status-gauge label is padded to, so the three bars start on the
/// same column however long each label is (the longest, XP, is 26).
const GAUGE_LABEL_WIDTH: usize = 28;

/// Draws the whole screen into `area`.
pub fn render(frame: &mut Frame, area: Rect, view: &View) {
    // Top to bottom: the strip, the panels, the gauges, a spacer that absorbs the
    // slack, then the footer pinned to the last row. Only the spacer flexes.
    let [haul_area, middle_area, gauges_area, _spacer, footer_area] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(12),
        Constraint::Length(3),
        Constraint::Min(0),
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

    // The right column splits into the Pickaxe panel over the Mine panel; the
    // Mine panel takes the slack so the two together fill the band exactly.
    let [pickaxe_area, mine_area] =
        Layout::vertical([Constraint::Length(6), Constraint::Min(0)]).areas(right_area);
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
fn gauge(frame: &mut Frame, area: Rect, label: &str, ratio: f64) {
    let gauge = LineGauge::default()
        .label(format!("{label:<GAUGE_LABEL_WIDTH$}"))
        .ratio(ratio)
        .filled_symbol("█")
        .unfilled_symbol("░")
        .filled_style(Style::default().fg(Color::White))
        .unfilled_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(gauge, area);
}

/// The footer line — the screen-local bindings; `q`/`s` stay off it by design.
fn footer(frame: &mut Frame, area: Rect) {
    let line = " Space  mine     Tab  next screen     ?  help";
    frame.render_widget(Paragraph::new(line), area);
}

/// A rounded, fully-bordered panel with the given title — the shape every boxed
/// element on this screen shares, spelled once so they cannot drift apart.
///
/// Returns `Block<'static>`, not `Block<'_>`: the block **owns** its title (that
/// `to_owned` is why), so tying the output's lifetime to the borrowed `title`
/// would make callers keep a temporary format string alive for nothing.
fn panel(title: &str) -> Block<'static> {
    Block::default()
        .title(title.to_owned())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
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
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use super::*;

    /// Renders the Mine screen alone into an 80×24 buffer.
    ///
    /// The screen is given the whole area — there is no tab bar in this harness —
    /// so its Haul strip lands at row 0, which is what the row arithmetic below
    /// counts from. Infallible `TestBackend` errors are discharged with an empty
    /// `match`, the same way `app.rs` does, rather than an `unwrap` the lints flag.
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

    /// The symbol at `(x, y)`.
    fn sym(buffer: &Buffer, x: u16, y: u16) -> &str {
        buffer[(x, y)].symbol()
    }

    /// Every row joined into one string — for "is this text on screen anywhere".
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
        let frame = whole_frame(&render_screen());
        // 12 × 7 = 84 cells, 8 of them holes in the sample grid, so 76 standing.
        assert!(frame.contains("Blocks    76 / 84"), "{frame}");
        assert!(frame.contains("Size      12 x 7"), "{frame}");
        assert!(frame.contains("(level 5)"), "{frame}");
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
}
