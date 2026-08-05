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
    crossterm::event::{KeyCode, KeyEvent},
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{LineGauge, Paragraph},
};
use skylode_core::block::Block;

use crate::{
    action::Action,
    format::{grouped, justified, shown_rung, xp_progress, xp_ratio},
    screen::panel,
    theme,
    view::{HaulEntry, HaulView, View},
    widget::MineGrid,
};

/// What a gauge reads when there is nothing for it to measure.
///
/// An em dash rather than a zero, and the distinction is not cosmetic: `Break 0%`
/// asserts that a block is being dug and is nought percent through, while the state
/// this stands for is that **no block is targeted at all**. The same glyph the
/// Upgrades mark column uses for a track with nothing to say.
const NOTHING: &str = "—";

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
    let block = panel(&format!(" Haul · {} ", view.mine_name));
    // The width *inside* the borders, which is what the line has to fit — asked of
    // the block rather than computed as `area.width - 2`, so a change of border
    // style cannot leave the arithmetic behind.
    let width = usize::from(block.inner(area).width);
    frame.render_widget(
        Paragraph::new(haul_line(&view.haul, width)).block(block),
        area,
    );
}

/// The strip's one content line, laid out to `width` — the two shapes, and why.
///
/// **A same-material mine keeps the wireframe's wording**, composite value and all:
/// `Iron   480 Raw   2 Compressed` with `value 680 Iron` after it. There is one
/// currency, so summing it answers "what is this mine worth to me" in one number.
///
/// **A two-material mine drops the total and gains a `·`.** Two segments, each with
/// its own denominations, and *no* composite — because there is no such thing as the
/// total of 540 Netherrack and 73 Quartz. Adding them would invent an exchange rate
/// the game does not have; quoting one and not the other would pick a favourite
/// between the material that funds the mine's growth and the one it exists to
/// produce. The three-row box is fixed (`organization/UI-EN.md` §5.2), so this has
/// one line whichever shape it takes, and the grid below never moves.
///
/// **The composite is flush right rather than eight spaces along**, which is the one
/// place this departs from the counted frame, and it is not a preference. The frame
/// was drawn at `480 Raw`; at the holdings a real run reaches, the fixed gap plus a
/// material name printed twice overflows the box — the Ancient Debris mine spends 28
/// of 78 columns on its own name before a digit — and a fixed-width box cannot wrap.
/// Anchoring the total to the right turns the gap into the slack, so the line grows
/// inward from both ends and the budget below is what falls out. At the frame's own
/// numbers every word is still there, in the same order.
///
/// The two-material shape is **tighter in two ways**: single spaces where the first
/// uses three, and *Compressed* shortened to `Comp.`. Both are bought width, and the
/// worst pair in the game — Obsidian beside Crying Obsidian, 23 columns of names —
/// does not fit the loose wording at any holding worth showing.
fn haul_line(haul: &HaulView, width: usize) -> String {
    let Some(value) = haul.value else {
        let entry = haul.common;
        let held = format!(
            "  {}   {} Raw   {} Compressed",
            entry.material,
            grouped(entry.raw),
            grouped(entry.compressed),
        );
        let composite = format!("value {} {}  ", grouped(entry.value()), entry.material);
        return justified(&held, &composite, width);
    };
    let segment = |entry: HaulEntry| {
        format!(
            "{} {} Raw {} Comp.",
            entry.material,
            grouped(entry.raw),
            grouped(entry.compressed),
        )
    };
    format!("  {} · {}", segment(haul.common), segment(value))
}

/// The holdings [`haul_line`] guarantees will fit the strip, as `(raw, compressed)`.
///
/// **A stated bound, not a discovered one.** The strip is a fixed three-row box on
/// every mine (`organization/UI-EN.md` §5.2) — that is what keeps the grid below it
/// from moving — so the line inside cannot wrap, and something has to give at some
/// number. Roughly six hundred thousand in raw value is where this design draws it.
///
/// **The odd-looking `499_999` is the Ancient Debris mine, and nothing else.** Its
/// material name is fourteen columns and the same-material line prints it twice —
/// once to open the line, once inside the composite — so 28 of the 78 available
/// columns are spent before a digit appears. Every other mine fits far more: Iron
/// clears the same holding with fourteen columns to spare. The bound is a
/// *guarantee across all twelve*, not a description of the typical line.
///
/// **Raw and Compressed are not independent, which is why one pair covers both.**
/// Compressed units are minted *out of* raw, a hundred at a time, so a player
/// holding 999 Compressed is by that fact 99 900 raw lighter — the corner where both
/// columns are simultaneously maxed is one the rules cannot reach.
///
/// A run can still exceed it by never compressing at all, since the denomination is
/// minted by hand. Past the bound ratatui clips the tail rather than reflowing, so
/// the box keeps its shape and the layout never breaks. If the ceiling ever needs
/// raising, the lever is the wording, not the box.
#[cfg(test)]
const HAUL_BUDGET: (u32, u32) = (499_999, 999);

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

    // The flash is chained unconditionally and the target is not, and the asymmetry is
    // the two widgets' own shapes: an empty map draws nothing, whereas "no target" has
    // to be the *absence* of the call so a break ratio cannot exist without a cell.
    let mut grid = MineGrid::new(view.mine_kind, &view.grid)
        .mode(view.colour_mode)
        .flash(&view.flash);
    // Chained conditionally rather than passed as an `Option`: the widget models
    // "nothing is being dug" as the absence of the call, so that a break ratio
    // cannot exist without a cell for it to be about — the same shape
    // [`TargetView`](crate::view::TargetView) now gives the view itself.
    if let Some(target) = view.target {
        grid = grid.target(target.cell, target.ratio);
    }
    frame.render_widget(grid, inner);
}

/// The Pickaxe panel: derived numbers only, not the full enchant roster (§5.1).
fn pickaxe_panel(frame: &mut Frame, area: Rect, view: &View) {
    let pickaxe = &view.pickaxe;
    // **The boost is shown only when there is one.** The product is worth printing
    // because `power × boost` is the number the player actually mines at — but with
    // no boost running that reads `25.0 ×1.0 boost → 25.0`, which is a
    // multiplication by one presented as news. The unboosted line is the bare power,
    // and the arrow appearing is itself the signal that something changed.
    let power = match &view.boost {
        Some(boost) => format!(
            " Power  {:.1}   ×{:.1} boost → {:.1}",
            pickaxe.power,
            boost.multiplier,
            pickaxe.power * boost.multiplier,
        ),
        None => format!(" Power  {:.1}", pickaxe.power),
    };
    let lines = vec![
        Line::from(format!(" {}", pickaxe.summary)),
        Line::from(power),
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
            shown_rung(panel_view.size_level),
        )),
        Line::from(format!(
            // `ceiling`, not `level`: this is the highest rung the dial may reach,
            // which is the Mines pane's and the Upgrades pane's word for it too.
            // `1/10` and not `1 / 10`: the spaced form is this pane's shape for a
            // *count* out of a total (`Blocks 31 / 40`), and this is a rung out of a
            // ladder — the same fact the Mines screen's dial prints as `1/10`. The tight
            // form is also the only one that fits: `ceiling` costs two columns more than
            // `level` did, and the spaced fraction pushed `value 10%` off a 36-column
            // pane by exactly one character.
            " Richness  ceiling {}/{}   value {}%",
            shown_rung(panel_view.richness_level),
            shown_rung(panel_view.richness_max),
            panel_view.value_percent,
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

    gauge(
        frame,
        break_area,
        &break_label(view),
        view.target.map_or(0.0, |t| ratio(f64::from(t.ratio))),
    );
    gauge(
        frame,
        xp_area,
        &format!(
            " XP  Lv {}   {}",
            view.player_level,
            xp_progress(view.xp, view.xp_to_next),
        ),
        ratio(xp_ratio(view.xp, view.xp_to_next)),
    );
    gauge(
        frame,
        boost_area,
        &match &view.boost {
            Some(boost) => format!(" Boost  {}s  ×{:.2}", boost.seconds, boost.multiplier),
            None => format!(" Boost  {NOTHING}"),
        },
        view.boost
            .as_ref()
            .map_or(0.0, |b| ratio(f64::from(b.ratio))),
    );
}

/// The Break gauge's label: `Break  61%  Iron Block`, or a dash with no target.
///
/// **The block's name is read off the grid, not stored beside the target.** The
/// target is a cell, and the cell already holds the block — so
/// [`Block::name`](skylode_core::block::Block::name) answers at the moment of
/// drawing and the label provably describes the crack the player is looking at. A
/// `target_name: String` on the view was the alternative, and it was a second copy
/// of a fact the grid already carried.
///
/// A target pointing outside the grid, or at a hole, falls back to the bare
/// percentage rather than refusing: the rules cannot produce either, but this is a
/// renderer and a frame that draws slightly less is a better failure than one that
/// does not draw.
fn break_label(view: &View) -> String {
    let Some(target) = view.target else {
        // Not `0%`. A percentage claims a block is partly broken; with nothing
        // targeted there is no block, and the dash says *that* instead.
        return format!(" Break  {NOTHING}");
    };
    let percent = (target.ratio * 100.0).round() as u32;
    let name = view
        .grid
        .get(usize::from(target.cell.1))
        .and_then(|row| row.get(usize::from(target.cell.0)))
        .and_then(|cell| cell.map(Block::name));
    match name {
        Some(name) => format!(" Break  {percent}%  {name}"),
        None => format!(" Break  {percent}%"),
    }
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
    // `b` earns its slot on the rule §9 states for `q`: a footer shows the bindings the
    // *screen* owns, and this is the only screen that owns this one. It is also the
    // only place it is advertised outside Help — the Upgrades pane that sells a charge
    // names the key, but a player who was granted one by a level-up never sees that
    // pane.
    let line = " Space  mine     b  boost     Tab  next screen     ?  help";
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(theme::MUTED)),
        area,
    );
}

/// A ratio clamped into the unit range that `LineGauge::ratio` demands.
///
/// `LineGauge::ratio` **panics** outside `0.0..=1.0`, and this crate forbids a
/// panic in production, so every ratio passes through here first. A non-finite
/// value maps to `0.0` rather than propagating a `NaN` that `clamp` would pass
/// straight through to the panic.
///
/// The `NaN` route is narrower than it was — `xp_to_next` is an [`Option`] now, so
/// the capped level yields [`format::xp_ratio`](crate::format::xp_ratio)'s `1.0`
/// instead of dividing by a sentinel — but this stays a *filter over every ratio*
/// and not a check at the one site that used to need it. A gauge that can panic is
/// a gauge that crashes the game to report a rounding error.
fn ratio(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// `Space` swings the pickaxe and `b` fires a boost charge (UI.md §9).
///
/// **Only the press half of `Space` lives here.** The release is answered in
/// [`keymap::resolve`](crate::keymap::resolve), which is the only place that still
/// knows a key's *kind* — this function is handed a bare key, so a release arriving
/// here would be indistinguishable from a press and would read as a second swing.
///
/// It emits [`Action::MinePressed`] and not "mine one block": whether the player is
/// mining is a question about *time*, answered by
/// [`App::advance`](crate::app::App::advance) from the instant this last arrived.
/// Auto-repeat is what keeps that instant fresh while the key is down, and it is why
/// a held `Space` needs no timer of its own here.
///
/// **`b` needs none of that**, and the contrast is worth keeping in view: a boost is
/// an *event*, not a state the terminal has to be interrogated about. One press, one
/// charge, one window the tick counts down on its own.
///
/// **A held `b` therefore fires repeatedly**, and that is not guarded against because
/// it cannot be: under the legacy encoding an auto-repeat is byte-for-byte a fresh
/// press, which is the same fact `Space` is built on. What keeps it harmless is the
/// stacking rule — a second charge *adds* its window rather than replacing it, so a
/// leaned-on key spends the reserve early but destroys none of it.
pub fn map_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char(' ') => Some(Action::MinePressed),
        KeyCode::Char('b') => Some(Action::FireBoost),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{
        Terminal, backend::TestBackend, buffer::Buffer, crossterm::event::KeyModifiers,
        style::Modifier,
    };
    use skylode_core::{mine_kind::MineKind, tunables::LEVEL_CAP};

    use super::*;
    use crate::{flash::FlashStage, view::TargetView};

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
        render_view(&View::sample(), width, height)
    }

    /// Renders an arbitrary view, for the states the fixture does not carry.
    ///
    /// The fixture is a level-23 save mid-swing, so it has a target, a boost and a
    /// next level — none of which a fresh run has. Taking the view as a parameter is
    /// what lets those three be asserted at all: the alternative is a second fixture
    /// that would have to be kept in step with this one.
    fn render_view(view: &View, width: u16, height: u16) -> Buffer {
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
        assert!(frame.contains("(level 10)"), "{frame}");
        assert!(
            frame.contains("Richness  ceiling 1/10   value 10%"),
            "{frame}"
        );
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
        // The one place `b` is advertised outside Help, and the only footer that owns
        // it — a charge granted by a level-up is otherwise a number with no key.
        assert!(last.contains("b  boost"), "{last:?}");
        assert!(last.contains("?  help"), "{last:?}");
        assert!(
            !last.contains(" q "),
            "a global key leaked into the footer: {last:?}"
        );
    }

    /// The screen's two keys, and nothing else claimed.
    ///
    /// **`b` is decoded here and not in [`crate::keymap`]**, which is what keeps it off
    /// the other five screens: a screen that does not claim a key declines it, and the
    /// global table never had it. The unbound letter is checked in the same test because
    /// "this screen answers `b`" and "this screen swallows the alphabet" are one line
    /// apart in the implementation.
    #[test]
    fn the_mine_screen_claims_space_and_b_and_declines_the_rest() {
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        assert_eq!(
            map_key(key(KeyCode::Char(' '))),
            Some(Action::MinePressed),
            "the mine key stopped answering"
        );
        assert_eq!(map_key(key(KeyCode::Char('b'))), Some(Action::FireBoost));
        assert_eq!(
            map_key(key(KeyCode::Char('B'))),
            None,
            "the shift is not it"
        );
        assert_eq!(map_key(key(KeyCode::Char('z'))), None);
        assert_eq!(map_key(key(KeyCode::Enter)), None);
    }

    /// The two-material strip prints both segments, and no composite total.
    ///
    /// The negative assertion carries the decision: there is no exchange rate
    /// between Netherrack and Quartz, so a `value N` on this line would be a number
    /// the game cannot mean.
    #[test]
    fn a_two_material_mine_prints_both_segments_and_no_total() {
        let view = View {
            mine_kind: MineKind::Quartz,
            mine_name: "Quartz Mine".to_owned(),
            haul: HaulView {
                common: HaulEntry {
                    material: "Netherrack",
                    raw: 340,
                    compressed: 2,
                },
                value: Some(HaulEntry {
                    material: "Quartz",
                    raw: 73,
                    compressed: 0,
                }),
            },
            ..View::sample()
        };
        let buffer = render_view(&view, 80, 24);
        // Row 1 is the strip's one content line, between its two border rows.
        let strip: String = (0..buffer.area.width)
            .map(|x| buffer[(x, 1)].symbol())
            .collect();
        assert!(strip.contains("Netherrack 340 Raw 2 Comp."), "{strip:?}");
        assert!(strip.contains("Quartz 73 Raw 0 Comp."), "{strip:?}");
        assert!(strip.contains(" · "), "the separator is missing: {strip:?}");
        // Read off the strip and not the whole frame: the Mine panel below carries
        // its own `value 10%` for the richness dial, so a frame-wide search would
        // find that and fail for a reason this test is not about.
        assert!(!strip.contains("value "), "a composite survived: {strip:?}");

        let frame = whole_frame(&buffer);
        assert!(frame.contains("Haul · Quartz Mine"), "{frame}");
    }

    /// Every mine's strip fits the box at [`HAUL_BUDGET`].
    ///
    /// Walks all twelve **through the core**, so the materials are the ones each
    /// mine really drops rather than a list retyped here — and so the three
    /// two-material mines are found by asking `common != value`, not by remembering
    /// which they are. The width is the strip's inside: 80 columns less two borders.
    ///
    /// Every entry is filled to the budget rather than to the fixture's numbers,
    /// because the line that matters is the **longest** one: a test at `480 Raw`
    /// would pass on a layout that overflows the first time a run gets rich. The
    /// pair that decides it is Obsidian beside Crying Obsidian — 23 columns of names
    /// before a digit is printed, and the reason the budget is where it is.
    #[test]
    fn every_mines_haul_line_fits_the_strip_at_the_stated_budget() {
        const INNER_WIDTH: usize = 78;
        let (raw, compressed) = HAUL_BUDGET;

        for kind in [
            MineKind::Stone,
            MineKind::Coal,
            MineKind::Iron,
            MineKind::Gold,
            MineKind::Lapis,
            MineKind::Redstone,
            MineKind::Emerald,
            MineKind::Diamond,
            MineKind::Quartz,
            MineKind::AncientDebris,
            MineKind::Obsidian,
            MineKind::Amethyst,
        ] {
            let common = HaulEntry {
                material: kind.common_material().name(),
                raw,
                compressed,
            };
            let value = (kind.value_material() != kind.common_material()).then_some(HaulEntry {
                material: kind.value_material().name(),
                raw,
                compressed,
            });
            let line = haul_line(&HaulView { common, value }, INNER_WIDTH);
            assert!(
                line.chars().count() <= INNER_WIDTH,
                "{kind:?}: {} columns — {line:?}",
                line.chars().count()
            );
        }
    }

    /// The composite is the two denominations added at the hundred-to-one rate.
    ///
    /// Asserted on [`HaulEntry::value`] rather than through the frame, because what
    /// is being checked is the arithmetic and not the layout: 2 Compressed is 200
    /// raw, so 480 + 200 is the 680 the wireframe prints.
    #[test]
    fn the_composite_value_counts_a_compressed_unit_as_a_hundred_raw() {
        let entry = HaulEntry {
            material: "Iron",
            raw: 480,
            compressed: 2,
        };
        assert_eq!(entry.value(), 680);
    }

    /// A run that has never swung has no target, and the gauge must say so.
    ///
    /// This is the state `cargo run` opens on until the phase-7 tick exists, so it
    /// is the *ordinary* case rather than an edge one. The two assertions are a pair:
    /// the dash appears **and** no percentage does, because `Break  0%` is the
    /// plausible-looking wrong answer this design rejected — it asserts a block is
    /// being dug when none is.
    #[test]
    fn a_run_that_has_not_swung_shows_a_dash_and_no_break_percentage() {
        let view = View {
            target: None,
            ..View::sample()
        };
        let frame = whole_frame(&render_view(&view, 80, 24));
        assert!(frame.contains("Break  —"), "{frame}");
        assert!(!frame.contains("Break  0%"), "{frame}");
        assert!(
            !frame.contains("Iron Block"),
            "a stale label survived: {frame}"
        );
    }

    /// The Break label names the block the target points at, read off the grid.
    ///
    /// Moving the target to a different *kind* of cell is what proves the name is
    /// derived rather than stored: the fixture's own target sits on an `IronBlock`,
    /// so pointing it at an `IronOre` cell must change the words on screen. A
    /// `target_name` field would have gone on saying "Iron Block".
    #[test]
    fn the_break_label_is_read_off_the_cell_the_target_points_at() {
        let sample = View::sample();
        // (0, 2) is an ore cell in every grid fixture's third row.
        assert_eq!(
            sample.grid.get(2).and_then(|row| row.first()),
            Some(&Some(Block::IronOre))
        );
        let view = View {
            target: Some(TargetView {
                cell: (0, 2),
                ratio: 0.5,
            }),
            ..View::sample()
        };
        let frame = whole_frame(&render_view(&view, 80, 24));
        assert!(frame.contains("Break  50%  Iron Ore"), "{frame}");
    }

    /// A target on a hole draws its percentage and drops the name, rather than
    /// refusing to draw.
    ///
    /// The rules cannot produce this — a dig targets a standing cell — but a
    /// renderer handed an impossible state should degrade, not disappear. Kept as a
    /// test because the fallback is invisible in ordinary play and would otherwise
    /// rot unexecuted.
    #[test]
    fn a_target_that_names_no_block_still_draws_its_percentage() {
        let sample = View::sample();
        // (6, 0) is a hole in the live 20×10 fixture.
        assert_eq!(sample.grid.first().and_then(|row| row.get(6)), Some(&None));
        let view = View {
            target: Some(TargetView {
                cell: (6, 0),
                ratio: 0.25,
            }),
            ..View::sample()
        };
        let frame = whole_frame(&render_view(&view, 80, 24));
        assert!(frame.contains("Break  25%"), "{frame}");
    }

    /// With no boost running, the power line is the bare power.
    ///
    /// `25.0 ×1.0 boost → 25.0` is the version this rejected: a multiplication by
    /// one, printed as though something were happening. The gauge below likewise
    /// dashes rather than counting down from zero.
    #[test]
    fn an_unboosted_pickaxe_prints_no_multiplication_and_no_countdown() {
        let view = View {
            boost: None,
            ..View::sample()
        };
        let frame = whole_frame(&render_view(&view, 80, 24));
        assert!(frame.contains("Power  25.0"), "{frame}");
        assert!(!frame.contains("boost →"), "the arrow survived: {frame}");
        assert!(frame.contains("Boost  —"), "{frame}");
        assert!(!frame.contains("Boost  0s"), "{frame}");
    }

    /// At the level cap the XP gauge reads `maxed` on a **full** bar.
    ///
    /// Both halves matter, and they are asserted together for the reason
    /// `format::xp_ratio` exists: a capped player has earned every level there is,
    /// so a word saying *finished* over an empty bar would be the screen
    /// contradicting itself.
    #[test]
    fn a_capped_player_reads_maxed_on_a_filled_xp_bar() {
        let view = View {
            player_level: LEVEL_CAP,
            xp_to_next: None,
            ..View::sample()
        };
        let buffer = render_view(&view, 80, 24);
        let frame = whole_frame(&buffer);
        assert!(
            frame.contains(&format!("XP  Lv {LEVEL_CAP}   maxed")),
            "{frame}"
        );

        // The XP gauge is the second of the three rows under the grid panel.
        let (_, grid_bottom) = grid_box_rows(&buffer);
        let y = grid_bottom + 2;
        assert!(
            (0..buffer.area.width).all(|x| buffer[(x, y)].symbol() != "░"),
            "the bar is not full at the cap"
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

    /// **A Nuke covers the whole grid, and the panel around it does not move.**
    ///
    /// The largest blast in the game is every cell of a maxed 20×10 mine, which is also
    /// the widest thing the 42-column panel ever holds — so this is where a flash that
    /// painted one column too far would show. It is the same invariant
    /// `the_grid_panel_is_forty_two_columns_wide` states, asked of the one frame that
    /// could break it.
    #[test]
    fn a_blast_over_the_whole_grid_leaves_the_panel_where_it_was() {
        let sample = View::sample();
        let flash = sample
            .grid
            .iter()
            .enumerate()
            .flat_map(|(y, row)| {
                (0..row.len()).map(move |x| ((x as u8, y as u8), FlashStage::Bright))
            })
            .collect();
        let view = View {
            flash,
            ..View::sample()
        };
        let buffer = render_view(&view, 80, 24);

        // The box is still a box, and the right column still opens where it did.
        assert_eq!(sym(&buffer, 0, 3), "╭");
        assert_eq!(sym(&buffer, 41, 3), "╮", "the blast widened the panel");
        assert_eq!(sym(&buffer, 42, 3), "╭");

        // Every one of the two hundred cells is flashing, and nothing outside them is:
        // the Pickaxe panel beside the grid keeps its own words.
        let blast = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buffer[(x, y)].bg == crate::palette::BLAST)
            .count();
        assert_eq!(blast, 20 * 10 * usize::from(crate::widget::CELL_WIDTH));
        assert!(whole_frame(&buffer).contains("Diamond Pickaxe"));
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
