//! Custom widgets that paint the buffer directly.
//!
//! Anything reusable across screens that ratatui does not already ship belongs
//! here; anything ratatui *does* ship (`List`, `Table`, `Tabs`, `LineGauge`)
//! should be used as-is rather than re-implemented.
//!
//! Today that is one widget: [`MineGrid`], the most repeated element in the game.

use ratatui::{buffer::Buffer, layout::Rect, style::Style, widgets::Widget};
use skylode_core::{block::Block, mine_kind::MineKind};

use crate::palette::{self, CellRole, ColourMode, Swatch};

/// How many columns one cell occupies. Two, everywhere, in both colour modes —
/// which is what lets a 20-wide mine fit the fixed 42-column panel (UI.md §5.1).
pub const CELL_WIDTH: u16 = 2;

/// The mark on a **value** cell, repeated across both its columns.
///
/// Unconditional in both colour modes, and that is load-bearing rather than
/// decorative: at 16 colours a mine has one colour, so this glyph is the *only*
/// thing separating common from value. Making it conditional would make the one
/// distinction the Mine screen exists for depend on a setting (UI.md §4.4).
const STIPPLE: &str = "░";

/// The crack progression on the targeted cell, filling in as it breaks.
///
/// `.:#` is MECHANICS' ordering, and it survives here because an intact cell now
/// carries **no glyph at all**: `#` is unclaimed, so ink accumulating on a bare
/// swatch reads the way that document wrote it down.
const CRACKS: [&str; 3] = ["·", ":", "#"];

/// The glyph for a target that is `ratio` of the way through.
///
/// Three equal bands. The wireframe in UI.md §5.1 draws `::` beside a `Break 61%`
/// gauge, which is what pins the thresholds to thirds rather than to any other
/// split.
///
/// The targeted cell is glyphed **from the very first frame**, at `ratio` 0, and
/// that is deliberate: the crack is also what marks the cell as the target, so a
/// band that drew nothing would leave a fresh aim invisible.
///
/// Total by construction, including for a `NaN` ratio: `clamp` passes `NaN`
/// through, `as usize` maps it to 0, and the `min` catches the exact-1.0 case
/// where `1.0 * 3.0` lands one past the last band. Returning a glyph rather than a
/// `Result` is right here — a renderer cannot refuse to draw.
fn crack(ratio: f32) -> &'static str {
    let band = (ratio.clamp(0.0, 1.0) * CRACKS.len() as f32) as usize;
    CRACKS[band.min(CRACKS.len() - 1)]
}

/// A mine's grid, painted as coloured swatches.
///
/// **Not a `Canvas`.** `Canvas` plots braille dot shapes over a floating-point
/// coordinate space; this is a character lattice where the *cell* is the unit, so
/// it writes into the [`Buffer`] itself.
///
/// **It borrows the grid rather than taking a `Mine`.** Two reasons, and the second
/// is the one that matters: a test has to be able to hand it a hand-built grid, and
/// `Mine::new` cannot produce one without an [`Rng`] and a seed — a golden test
/// would then be asserting against whatever the generator drew, which is a test of
/// the core wearing a renderer's clothes. Borrowing also keeps the frame free of a
/// per-redraw clone of up to 200 cells.
///
/// [`Rng`]: skylode_core::rng::Rng
#[derive(Clone, Copy, Debug)]
pub struct MineGrid<'a> {
    /// Which mine this is — the only thing that answers what colour a cell takes.
    kind: MineKind,
    /// The grid itself, in `Mine::get_grid`'s shape: `None` is a hole.
    grid: &'a [Vec<Option<Block>>],
    /// The cell being dug and how far through it is, or `None` when nothing is
    /// aimed at.
    ///
    /// **One field for both**, because a break ratio without a target is a number
    /// about nothing. Fusing them makes that state unrepresentable rather than
    /// merely unlikely — the same move `Mine::grid` makes by fusing its hole mask
    /// into `Option<Block>`.
    target: Option<((usize, usize), f32)>,
    /// How many colours to ask the terminal for.
    mode: ColourMode,
}

impl<'a> MineGrid<'a> {
    /// A grid with nothing targeted, at the default colour mode.
    ///
    /// The builder methods below follow ratatui's own widget convention — `self`
    /// by value, returning `Self` — so a call site reads as one expression and the
    /// optional parts are visibly optional.
    pub fn new(kind: MineKind, grid: &'a [Vec<Option<Block>>]) -> Self {
        Self {
            kind,
            grid,
            target: None,
            mode: ColourMode::default(),
        }
    }

    /// Marks `cell` as the one being dug, `ratio` of the way through.
    ///
    /// Takes the cell by value rather than as an `Option`: "not digging" is the
    /// absence of this call, so the two arguments cannot be supplied apart.
    #[must_use]
    pub fn target(mut self, cell: (u8, u8), ratio: f32) -> Self {
        self.target = Some(((usize::from(cell.0), usize::from(cell.1)), ratio));
        self
    }

    /// Draws at 16 colours instead of 256.
    #[must_use]
    pub fn mode(mut self, mode: ColourMode) -> Self {
        self.mode = mode;
        self
    }

    /// The swatch and the glyph one standing block should be drawn with.
    ///
    /// Any block that is not the mine's [value block](MineKind::value_block) is
    /// treated as common. That keeps the function **total**: today `draw_cell`
    /// only ever puts one of the mine's own two blocks in the grid, but a renderer
    /// that panicked the day that stopped being true would take the process down
    /// over a colour.
    fn paint(&self, block: Block, is_target: bool, ratio: f32) -> (Swatch, Option<&'static str>) {
        let role = if block == self.kind.value_block() {
            CellRole::Value
        } else {
            CellRole::Common
        };
        let swatch = palette::swatch(self.kind, role, self.mode);

        // The stipple and the crack never collide, because only one cell is ever
        // mid-break: on every other cell the glyph channel is idle and free to
        // carry the value mark (UI.md §4.1).
        let glyph = if is_target {
            Some(crack(ratio))
        } else if role == CellRole::Value {
            Some(STIPPLE)
        } else {
            None
        };

        (swatch, glyph)
    }
}

impl Widget for MineGrid<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let rows = self.grid.len() as u16;
        // Rectangular by construction — `Mine::reset` builds every row at one
        // width — so the first row's length is the grid's.
        let columns = self.grid.first().map_or(0, Vec::len) as u16 * CELL_WIDTH;

        // Centred in whatever it is given, which is what the frame in UI.md §5.1
        // draws: a 12-wide grid is 24 columns inside a 40-column panel, with eight
        // of margin either side. `saturating_sub` is the degenerate case — an area
        // too small to hold the grid pins the offset at zero and the clipping below
        // takes over, rather than wrapping the offset around to enormous.
        let left = area.x + area.width.saturating_sub(columns) / 2;
        let top = area.y + area.height.saturating_sub(rows) / 2;

        let target = self.target.map(|(cell, _)| cell);
        let ratio = self.target.map_or(0.0, |(_, ratio)| ratio);

        for (row_index, row) in self.grid.iter().enumerate() {
            let y = top.saturating_add(row_index as u16);
            if y >= area.bottom() {
                break;
            }

            for (column_index, cell) in row.iter().enumerate() {
                // A hole is the *absence* of a swatch — the terminal's own
                // background, which is the maximum contrast available against
                // every intact cell and needs no glyph of its own (UI.md §4.1).
                // Leaving the buffer untouched is how "absence" is spelled.
                let Some(block) = cell else { continue };

                let is_target = target == Some((column_index, row_index));
                let (swatch, glyph) = self.paint(*block, is_target, ratio);

                let style = match glyph {
                    Some(_) => Style::default().bg(swatch.bg).fg(swatch.ink),
                    None => Style::default().bg(swatch.bg),
                };

                let x = left.saturating_add(column_index as u16 * CELL_WIDTH);
                for offset in 0..CELL_WIDTH {
                    let column = x.saturating_add(offset);
                    // Clipped against `area` and not merely against the buffer: a
                    // widget that overran its own rectangle would paint over its
                    // neighbours instead of being cut off by them.
                    if column >= area.right() {
                        break;
                    }
                    // `cell_mut` and not `buf[(x, y)]`: the indexing form panics
                    // off the end of the buffer, and this crate warns on `panic`.
                    if let Some(target_cell) = buf.cell_mut((column, y)) {
                        target_cell.set_style(style);
                        target_cell.set_symbol(glyph.unwrap_or(" "));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::*;

    /// The Iron mine's two blocks, which every fixture below is built from.
    const COMMON: Option<Block> = Some(Block::IronOre);
    const VALUE: Option<Block> = Some(Block::IronBlock);
    const HOLE: Option<Block> = None;

    /// Iron's colours, spelled out here rather than read from the palette: a
    /// golden test that fetched its expectations from the code under test would
    /// pass no matter what the palette said.
    const IRON_COMMON_BG: Color = Color::Indexed(94);
    const IRON_COMMON_INK: Color = Color::Indexed(231);
    const IRON_VALUE_BG: Color = Color::Indexed(223);
    const IRON_VALUE_INK: Color = Color::Indexed(16);

    /// A 4x3 Iron grid with two value cells, two holes, and room for a target.
    fn fixture() -> Vec<Vec<Option<Block>>> {
        vec![
            vec![COMMON, VALUE, COMMON, HOLE],
            vec![COMMON, COMMON, HOLE, VALUE],
            vec![HOLE, COMMON, COMMON, COMMON],
        ]
    }

    /// Renders into a bare buffer of exactly the grid's size, so no centring
    /// offset is in play and the assertions are about colour, not arithmetic.
    fn render(widget: MineGrid<'_>, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        widget.render(area, &mut buffer);
        buffer
    }

    #[test]
    fn the_crack_fills_in_three_equal_bands() {
        assert_eq!(crack(0.0), "·");
        assert_eq!(crack(0.32), "·");
        assert_eq!(crack(0.34), ":");
        // The wireframe's own number, which is what pins the bands to thirds.
        assert_eq!(crack(0.61), ":");
        assert_eq!(crack(0.66), ":");
        assert_eq!(crack(0.67), "#");
        assert_eq!(crack(1.0), "#");
    }

    #[test]
    fn a_ratio_outside_the_unit_range_still_draws_something() {
        // `dig` clamps, so these are unreachable through the rules — but a
        // renderer is the wrong place to discover that a caller was wrong.
        assert_eq!(crack(-5.0), "·");
        assert_eq!(crack(12.0), "#");
        assert_eq!(crack(f32::NAN), "·");
    }

    #[test]
    fn a_known_grid_paints_the_expected_cells() {
        let grid = fixture();
        let buffer = render(
            MineGrid::new(MineKind::Iron, &grid).target((1, 1), 0.61),
            8,
            3,
        );

        // Two columns per cell: a common cell is a bare swatch, a value cell
        // carries the stipple, the target carries its crack, and a hole is blank.
        let mut expected = Buffer::with_lines(["  ░░    ", "  ::  ░░", "        "]);
        let common = Style::default().bg(IRON_COMMON_BG);
        let value = Style::default().bg(IRON_VALUE_BG).fg(IRON_VALUE_INK);
        let target = Style::default().bg(IRON_COMMON_BG).fg(IRON_COMMON_INK);
        for (x, y, style) in [
            (0, 0, common),
            (2, 0, value),
            (4, 0, common),
            (0, 1, common),
            (2, 1, target),
            (6, 1, value),
            (2, 2, common),
            (4, 2, common),
            (6, 2, common),
        ] {
            expected.set_style(Rect::new(x, y, CELL_WIDTH, 1), style);
        }

        assert_eq!(buffer, expected);
    }

    #[test]
    fn a_broken_cell_is_the_absence_of_a_swatch() {
        let grid = fixture();
        let buffer = render(MineGrid::new(MineKind::Iron, &grid), 8, 3);

        // (3, 0) in grid coordinates is a hole, so columns 6 and 7 of row 0 must
        // still be the terminal's own background rather than any block's colour.
        for column in 6..8 {
            let cell = &buffer[(column, 0)];
            assert_eq!(cell.bg, Color::Reset, "column {column} kept a swatch");
            assert_eq!(cell.symbol(), " ");
        }
    }

    #[test]
    fn the_value_cell_keeps_its_stipple_in_sixteen_colours() {
        let grid = fixture();
        let buffer = render(
            MineGrid::new(MineKind::Iron, &grid).mode(ColourMode::Ansi16),
            8,
            3,
        );

        let (common, value) = (&buffer[(0, 0)], &buffer[(2, 0)]);

        // The mine has one colour now, so the glyph is carrying the distinction
        // entirely — which is the whole claim of UI.md §4.3.
        assert_eq!(common.bg, value.bg);
        assert_eq!(common.symbol(), " ");
        assert_eq!(value.symbol(), STIPPLE);
    }

    #[test]
    fn the_grid_is_centred_in_the_area_it_is_given() {
        let grid = fixture();
        // 8 columns of grid in 12, 3 rows in 5: two columns and one row of margin.
        let buffer = render(MineGrid::new(MineKind::Iron, &grid), 12, 5);

        let margin = &buffer[(0, 0)];
        assert_eq!(margin.bg, Color::Reset, "the margin was painted");

        let first = &buffer[(2, 1)];
        assert_eq!(
            first.bg, IRON_COMMON_BG,
            "the grid is not where it should be"
        );
    }

    #[test]
    fn a_grid_larger_than_its_area_is_clipped_not_fatal() {
        // The largest mine in the game, into one cell of a wider buffer. Nothing
        // may be painted outside the area, and nothing may panic.
        let grid = vec![vec![COMMON; 20]; 10];
        let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 4));
        MineGrid::new(MineKind::Iron, &grid).render(Rect::new(0, 0, 1, 1), &mut buffer);

        assert_eq!(buffer[(0, 0)].bg, IRON_COMMON_BG);

        let outside = &buffer[(1, 0)];
        assert_eq!(outside.bg, Color::Reset, "the widget overran its own area");
    }

    #[test]
    fn an_empty_grid_draws_nothing_and_does_not_divide_by_its_own_size() {
        let grid: Vec<Vec<Option<Block>>> = Vec::new();
        let buffer = render(MineGrid::new(MineKind::Stone, &grid), 8, 3);
        assert_eq!(buffer, Buffer::empty(Rect::new(0, 0, 8, 3)));
    }
}
