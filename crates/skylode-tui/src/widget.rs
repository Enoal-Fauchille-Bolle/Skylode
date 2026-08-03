//! Custom widgets that paint the buffer directly.
//!
//! Anything reusable across screens that ratatui does not already ship belongs
//! here; anything ratatui *does* ship (`List`, `Table`, `Tabs`, `LineGauge`)
//! should be used as-is rather than re-implemented.
//!
//! Today that is one widget: [`MineGrid`], the most repeated element in the game.

use std::collections::BTreeMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};
use skylode_core::{block::Block, mine_kind::MineKind};

use crate::{
    flash::FlashStage,
    palette::{self, CellRole, ColourMode, Swatch},
};

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

/// The first beat of a proc flash: a **solid** block, in the blast colour, on the blast
/// colour (UI.md §7).
///
/// The glyph is not decoration on top of the background — it is what makes the flash
/// survive §4.4's rule that colour never carries an answer alone. Painted as background
/// *and* foreground, the cell reads as one solid lozenge on a colour terminal and still
/// fills completely on one that dropped the hue, where a background-only blast would be
/// invisible rather than merely subtle.
const BLAST_FULL: &str = "█";

/// The second beat: **half the ink**, on the terminal's own background.
///
/// This is how a text terminal spells "dimmed" — by coverage, not by luminance. The two
/// alternatives were both worse: a second, darker background costs a second named colour
/// at 16, where the twelve mines already spend seven of the eight; and `Modifier::DIM` is
/// rejected twice over — [`theme`](crate::theme) already refuses it because terminals
/// disagree about implementing it at all, and it applies to the *foreground*, so a cell
/// whose information is a background would not dim in the least.
///
/// `▒` and not `░`: the lighter shade is already the value cell's stipple and the
/// unfilled half of every gauge, and a flash must not be readable as either.
const BLAST_FADE: &str = "▒";

/// A flash with nothing in it, for [`MineGrid::new`] to borrow.
///
/// The widget holds the map by reference to stay [`Copy`] and to avoid cloning up to two
/// hundred entries per redraw, so "no flash" needs an actual map somewhere to point at.
/// [`BTreeMap::new`] is a `const fn`, which is what lets that somewhere be a `static`
/// rather than a field the caller has to supply.
static NO_FLASH: BTreeMap<(u8, u8), FlashStage> = BTreeMap::new();

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
    /// Which cells a spatial blast has claimed this frame, and how bright (UI.md §7).
    ///
    /// **Already resolved to a beat**, not to an instant: the widget is handed what to
    /// draw, and the wall clock stays in [`Flashes::resolve`](crate::flash::Flashes) one
    /// level up. A `MineGrid` that read a clock could not be golden-tested at all.
    flash: &'a BTreeMap<(u8, u8), FlashStage>,
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
            flash: &NO_FLASH,
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

    /// Paints `flash`'s cells as a spatial blast instead of as grid cells (UI.md §7).
    ///
    /// Takes the map by reference, and the map is the whole interface: *last blast wins
    /// per cell* was already decided by whoever built it, so there is nothing here to
    /// composite and no order to resolve. §7 asks for exactly that — *"no queue, no
    /// compositing rules"*.
    #[must_use]
    pub fn flash(mut self, flash: &'a BTreeMap<(u8, u8), FlashStage>) -> Self {
        self.flash = flash;
        self
    }

    /// The style and glyph a flashed cell takes on `beat`.
    ///
    /// **Both channels on both beats**, which is the redundancy of §4.4 rather than a
    /// flourish: the shape has to survive a terminal that dropped the hue, and a `bg`
    /// alone would not. `Color::Reset` is named explicitly on the fade rather than left
    /// out — the cell *is* a hole by now, and stating it means the beat does not depend
    /// on the frame buffer having been cleared first.
    fn blast(&self, beat: FlashStage) -> (Style, &'static str) {
        let colour = palette::blast(self.mode);
        match beat {
            FlashStage::Bright => (Style::default().bg(colour).fg(colour), BLAST_FULL),
            FlashStage::Fading => (Style::default().bg(Color::Reset).fg(colour), BLAST_FADE),
        }
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
                // **The flash is asked before the hole check, and that is the whole
                // trick.** By the time this widget sees a blast, the tick has already
                // broken every cell in it — so a lookup made *after* the `continue`
                // below would find nothing to paint and the flash would never draw at
                // all. "The cells are painted before they are erased" (UI.md §7) is, in
                // code, exactly these three lines sitting above the next three.
                //
                // The coordinates are narrowed rather than cast: the core quotes a cell
                // as `(u8, u8)`, so a grid wider than 255 could hold no flashed cell by
                // construction, and a truncating `as` would instead fold column 256 onto
                // column 0 and paint a blast in the wrong place.
                let beat = u8::try_from(column_index)
                    .ok()
                    .zip(u8::try_from(row_index).ok())
                    .and_then(|coordinates| self.flash.get(&coordinates));

                let (style, glyph) = match beat {
                    Some(&beat) => {
                        let (style, glyph) = self.blast(beat);
                        (style, Some(glyph))
                    }
                    None => {
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
                        (style, glyph)
                    }
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

    /// The blast colour, spelled out here rather than read from [`palette::blast`], for
    /// [`IRON_COMMON_BG`]'s reason: a test fetching its expectations from the code under
    /// test passes whatever that code says.
    const BLAST_BG: Color = Color::Indexed(202);

    /// A flash over the cells named, all on the same beat.
    fn flashing(beat: FlashStage, cells: &[(u8, u8)]) -> BTreeMap<(u8, u8), FlashStage> {
        cells.iter().map(|&cell| (cell, beat)).collect()
    }

    /// **The check the whole feature turns on.** By the time a flash is drawn the tick
    /// has already broken those cells, so the shape is painted over *holes* — and a
    /// widget that asked the flash after its `continue` would draw nothing at all.
    ///
    /// `(3, 0)` is a hole in the fixture, which is what makes this the real case rather
    /// than a contrived one.
    #[test]
    fn a_blast_is_painted_over_the_holes_it_has_just_made() {
        let grid = fixture();
        assert_eq!(grid[0][3], HOLE, "the fixture stopped having a hole here");
        let flash = flashing(FlashStage::Bright, &[(3, 0)]);

        let buffer = render(MineGrid::new(MineKind::Iron, &grid).flash(&flash), 8, 3);

        for column in 6..8 {
            let cell = &buffer[(column, 0)];
            assert_eq!(cell.bg, BLAST_BG, "column {column} was left as a hole");
            assert_eq!(cell.symbol(), BLAST_FULL);
        }
    }

    /// The two beats are two different pictures, and the difference is not the hue.
    ///
    /// Beat one owns both channels — a solid block on the blast colour — so it survives
    /// a terminal that dropped the colour entirely. Beat two hands the background back
    /// and keeps only the ink, which is how a text terminal spells "half as much of it".
    #[test]
    fn the_two_beats_differ_in_coverage_and_not_only_in_colour() {
        let grid = fixture();

        let bright = flashing(FlashStage::Bright, &[(0, 0)]);
        let buffer = render(MineGrid::new(MineKind::Iron, &grid).flash(&bright), 8, 3);
        let cell = &buffer[(0, 0)];
        assert_eq!((cell.bg, cell.fg), (BLAST_BG, BLAST_BG));
        assert_eq!(cell.symbol(), BLAST_FULL);

        let fading = flashing(FlashStage::Fading, &[(0, 0)]);
        let buffer = render(MineGrid::new(MineKind::Iron, &grid).flash(&fading), 8, 3);
        let cell = &buffer[(0, 0)];
        assert_eq!(
            (cell.bg, cell.fg),
            (Color::Reset, BLAST_BG),
            "the fade kept a background"
        );
        assert_eq!(cell.symbol(), BLAST_FADE);
    }

    /// **A flash outranks a block standing under it**, and this is not a corner case: a
    /// blast that empties the grid is refilled by the *same* swing
    /// (`Mine::refill_if_empty`), so on a small mine the cells are whole again before
    /// the first frame of the flash is drawn. Recorded as a departure in `docs/UI.md`
    /// §7.1 — §7's table says the cells are empty afterwards, and there they are not.
    ///
    /// The flash wins because the shape is the reward: what the player is owed is the
    /// picture of what just happened, and the refill has its own announcement.
    #[test]
    fn a_flash_outranks_a_block_that_is_standing_again_under_it() {
        let grid = fixture();
        assert_eq!(grid[0][0], COMMON, "the fixture stopped standing here");
        let flash = flashing(FlashStage::Bright, &[(0, 0)]);

        let buffer = render(MineGrid::new(MineKind::Iron, &grid).flash(&flash), 8, 3);

        assert_eq!(
            buffer[(0, 0)].bg,
            BLAST_BG,
            "the swatch outranked the blast"
        );
    }

    /// It outranks the crack too — reachable by the same refill, where the new target is
    /// drawn from cells the blast is still flashing.
    ///
    /// The crack is a hundred milliseconds late rather than contradicted, which is the
    /// right way round: a blast is news and a break percentage is a gauge, and the gauge
    /// beside the grid goes on saying it either way.
    #[test]
    fn a_flash_outranks_the_crack_on_the_target_under_it() {
        let grid = fixture();
        let flash = flashing(FlashStage::Bright, &[(1, 1)]);

        let buffer = render(
            MineGrid::new(MineKind::Iron, &grid)
                .target((1, 1), 0.61)
                .flash(&flash),
            8,
            3,
        );

        assert_eq!(buffer[(2, 1)].symbol(), BLAST_FULL, "the crack survived");
    }

    /// Two blasts inside one window put **both beats on one frame**, per cell.
    ///
    /// This is the only picture that separates "last blast wins per cell" from "the
    /// newest blast wins": a design that kept one flash at a time would draw this frame
    /// in a single beat and pass every other test here.
    #[test]
    fn one_frame_can_carry_both_beats_at_once() {
        let grid = fixture();
        let mut flash = flashing(FlashStage::Fading, &[(0, 0), (1, 0)]);
        flash.extend(flashing(FlashStage::Bright, &[(2, 0)]));

        let buffer = render(MineGrid::new(MineKind::Iron, &grid).flash(&flash), 8, 3);

        assert_eq!(buffer[(0, 0)].symbol(), BLAST_FADE);
        assert_eq!(buffer[(2, 0)].symbol(), BLAST_FADE);
        assert_eq!(buffer[(4, 0)].symbol(), BLAST_FULL);
    }

    /// At 16 colours the blast takes the one bright colour no mine claims, and the
    /// glyphs do not change at all — one rendering model with a channel switched off,
    /// which is the same claim §4.3 makes for the grid itself.
    #[test]
    fn the_blast_takes_a_named_colour_at_sixteen() {
        let grid = fixture();
        let flash = flashing(FlashStage::Bright, &[(0, 0)]);

        let buffer = render(
            MineGrid::new(MineKind::Iron, &grid)
                .mode(ColourMode::Ansi16)
                .flash(&flash),
            8,
            3,
        );

        let cell = &buffer[(0, 0)];
        assert_eq!(cell.bg, Color::LightRed);
        assert_eq!(cell.symbol(), BLAST_FULL);
        // And it is not the colour the mine itself is wearing, which is the whole
        // requirement at 16: Iron falls back to yellow.
        assert_ne!(cell.bg, buffer[(6, 2)].bg);
    }

    /// A flashed cell the grid does not have is never consulted, so a mine that shrank
    /// under a live flash cannot paint outside itself.
    ///
    /// Total by construction rather than by a guard: the widget walks the *grid* and
    /// looks the flash up, so an entry with no cell to belong to has nothing to be found
    /// by.
    #[test]
    fn a_flash_outside_the_grid_paints_nothing() {
        let grid = fixture();
        let flash = flashing(FlashStage::Bright, &[(19, 9)]);

        let buffer = render(MineGrid::new(MineKind::Iron, &grid).flash(&flash), 8, 3);

        let plain = render(MineGrid::new(MineKind::Iron, &grid), 8, 3);
        assert_eq!(buffer, plain, "a flash off the grid changed the frame");
    }

    #[test]
    fn an_empty_grid_draws_nothing_and_does_not_divide_by_its_own_size() {
        let grid: Vec<Vec<Option<Block>>> = Vec::new();
        let buffer = render(MineGrid::new(MineKind::Stone, &grid), 8, 3);
        assert_eq!(buffer, Buffer::empty(Rect::new(0, 0, 8, 3)));
    }
}
