//! The colour of a mine cell, in both colour modes.
//!
//! Twenty-four [`Block`](skylode_core::block::Block) variants have to be
//! recognisable at two columns wide, and [UI.md §4.2] settles how: the twelve
//! [`MineKind`] `(common, value)` pairs partition the block enum exactly, and
//! **two mines are never on screen together**, so the requirement is not
//! twenty-four mutually distinguishable colours — it is twelve pairs with strong
//! *internal* contrast. The gate that table was measured against, in CIELAB, is
//! `ΔE ≥ 40` and `ΔL* ≥ 20` per pair, with every swatch at `L* ≥ 12` so none of
//! them reads as a hole in the grid. This module's tests re-derive all three from the
//! constants below, because the script that first checked them is long gone and a
//! mistyped hex digit is otherwise invisible. (The tests are named rather than
//! linked throughout this module: rustdoc does not compile `#[cfg(test)]`
//! modules, so an intra-doc link to one never resolves.)
//!
//! ## Why the table is keyed by mine and not by block
//!
//! The 16-colour fallback assigns **one colour per mine**, not per block — sixteen
//! ANSI colours cannot carry twenty-four materials, so there the stipple alone
//! separates common from value. A table keyed by `Block` would therefore need a
//! reverse `Block -> MineKind` lookup to answer the fallback at all. Keyed by mine,
//! the question never arises: the widget is always drawing one mine, so it always
//! knows which row to read. The pairwise gate lands the same way — the two colours
//! a test has to compare sit on one row, in front of the reader.
//!
//! ## Why the lookup goes through a `match`
//!
//! [`MineKind`] is a dataless enum, so `PALETTE[kind as usize]` would compile and
//! work. It would also turn a thirteenth mine into an **out-of-range index at
//! runtime** — a panic on the player's machine, in the one function every frame
//! calls two hundred times. The `match` in [`palette`] turns that same omission
//! into a compile error, which is the trade this workspace makes everywhere:
//! `Mine::grid` fuses its hole mask into `Option<Block>` for the same reason.
//!
//! [UI.md §4.2]: ../../../docs/UI.md

use ratatui::style::Color;
use serde::{Deserialize, Serialize};
use skylode_core::mine_kind::MineKind;

/// Ink for a **light** swatch, as a 256-colour index rather than [`Color::Black`].
///
/// Index 16 is true `#000000`, while the named ANSI colours are whatever the
/// user's theme remapped them to — a Solarized terminal draws "black" as a dark
/// teal. The contrast figures this module asserts were computed against true
/// black and true white, so the constants have to name those exactly, or the test
/// would be measuring a colour the terminal never shows.
const INK_ON_LIGHT: Color = Color::Indexed(16);

/// Ink for a **dark** swatch: true `#ffffff`. See [`INK_ON_LIGHT`].
const INK_ON_DARK: Color = Color::Indexed(231);

/// One cell's two channels: the background it is painted in, and the colour any
/// glyph written on it takes.
///
/// The ink is stored rather than derived because no single ink works: black
/// vanishes on `CoalBlock` (`#262626`) and white on `GoldBlock` (`#ffff00`). It is
/// picked once per swatch, from that swatch's own lightness, and the
/// `every_swatch_can_be_written_on` test checks the pick rather than trusting it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Swatch {
    /// The two-column background — what the player actually reads as "a block".
    pub bg: Color,
    /// Foreground for the stipple and the crack glyph, chosen to survive `bg`.
    pub ink: Color,
}

impl Swatch {
    /// A light 256-colour swatch, which therefore takes dark ink.
    ///
    /// The two constructors exist so that the ink decision is **visible on every
    /// row** of [`PALETTE`]: a reader scanning the table sees `light` or `dark` and
    /// knows what will be written on it, instead of finding a second colour column
    /// that has to be checked against the first.
    const fn light(index: u8) -> Self {
        Self {
            bg: Color::Indexed(index),
            ink: INK_ON_LIGHT,
        }
    }

    /// A dark 256-colour swatch, which therefore takes light ink.
    const fn dark(index: u8) -> Self {
        Self {
            bg: Color::Indexed(index),
            ink: INK_ON_DARK,
        }
    }

    /// A light **named ANSI** swatch, for the 16-colour fallback.
    ///
    /// Named and not indexed, unlike [`light`](Self::light): the whole point of the
    /// fallback is to ask the terminal for its own sixteen colours, so remapping by
    /// the user's theme is the intended behaviour here rather than the hazard it is
    /// in 256-colour mode.
    const fn ansi_light(bg: Color) -> Self {
        Self {
            bg,
            ink: Color::Black,
        }
    }

    /// A dark named ANSI swatch. See [`ansi_light`](Self::ansi_light).
    const fn ansi_dark(bg: Color) -> Self {
        Self {
            bg,
            ink: Color::White,
        }
    }
}

/// Everything the renderer needs to colour one mine's cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinePalette {
    /// Which mine this row describes. Carried in the row so the table can be
    /// walked in tests without a second list to keep in step with it.
    pub mine: MineKind,
    /// The mine's [common block](MineKind::common_block), at 256 colours.
    pub common: Swatch,
    /// The mine's [value block](MineKind::value_block), at 256 colours.
    pub value: Swatch,
    /// The whole mine, at 16 colours — **both** roles share it, and the stipple is
    /// what still tells them apart.
    pub fallback: Swatch,
}

/// The twelve rows, in [`MineKind`] declaration order.
///
/// Transcribed from [UI.md §4.2]'s table, which chose them on one principle:
/// **Minecraft's hue, this game's lightness**. The hue is fidelity — a player
/// coming from SkyMines reads blue as Lapis without being taught — while the
/// lightness inside a pair is the free variable, because it is the only channel
/// that survives both a poor terminal and a colour-vision deficiency. Where the two
/// fought, lightness won: `RedstoneBlock` is `#ff8787` and not a faithful pure red,
/// which measured `ΔE 37` against Redstone Ore and missed the gate.
///
/// Black is absent from the fallback column on purpose: black is what a **broken**
/// cell looks like, so a mine painted in it would read as a hole.
///
/// [UI.md §4.2]: ../../../docs/UI.md
const PALETTE: [MinePalette; 12] = [
    // --- Overworld ---
    MinePalette {
        mine: MineKind::Stone,
        common: Swatch::dark(240), // #585858, L* 37
        value: Swatch::light(252), // #d0d0d0, L* 84
        fallback: Swatch::ansi_light(Color::White),
    },
    MinePalette {
        mine: MineKind::Coal,
        common: Swatch::light(246), // #949494, L* 61
        value: Swatch::dark(235),   // #262626, L* 15 — the darkest swatch in the game
        fallback: Swatch::ansi_light(Color::White),
    },
    MinePalette {
        mine: MineKind::Iron,
        common: Swatch::dark(94),  // #875f00, L* 43
        value: Swatch::light(223), // #ffd7af, L* 88
        fallback: Swatch::ansi_light(Color::Yellow),
    },
    MinePalette {
        mine: MineKind::Gold,
        common: Swatch::dark(130), // #af5f00, L* 49
        value: Swatch::light(226), // #ffff00, L* 97
        fallback: Swatch::ansi_light(Color::Yellow),
    },
    MinePalette {
        mine: MineKind::Lapis,
        common: Swatch::dark(20), // #0000d7, L* 27
        value: Swatch::light(81), // #5fd7ff, L* 81
        fallback: Swatch::ansi_dark(Color::Blue),
    },
    MinePalette {
        mine: MineKind::Redstone,
        common: Swatch::dark(88),  // #870000, L* 27
        value: Swatch::light(210), // #ff8787, L* 70
        fallback: Swatch::ansi_dark(Color::Red),
    },
    MinePalette {
        mine: MineKind::Emerald,
        common: Swatch::dark(29), // #00875f, L* 50
        value: Swatch::light(48), // #00ff87, L* 89
        fallback: Swatch::ansi_light(Color::Green),
    },
    MinePalette {
        mine: MineKind::Diamond,
        common: Swatch::light(30), // #008787, L* 51 — light by a whisker, and it holds
        value: Swatch::light(51),  // #00ffff, L* 91
        fallback: Swatch::ansi_light(Color::Cyan),
    },
    // --- Nether ---
    MinePalette {
        mine: MineKind::Quartz,
        common: Swatch::dark(95),  // #875f5f, L* 45
        value: Swatch::light(255), // #eeeeee, L* 94
        fallback: Swatch::ansi_dark(Color::Red),
    },
    MinePalette {
        mine: MineKind::AncientDebris,
        common: Swatch::light(137), // #af875f, L* 59
        value: Swatch::dark(238),   // #444444, L* 29
        fallback: Swatch::ansi_light(Color::Yellow),
    },
    MinePalette {
        mine: MineKind::Obsidian,
        common: Swatch::dark(54),  // #5f0087, L* 24
        value: Swatch::light(165), // #d700ff, L* 54
        fallback: Swatch::ansi_dark(Color::Magenta),
    },
    // --- End ---
    MinePalette {
        mine: MineKind::Amethyst,
        common: Swatch::light(229), // #ffffaf, L* 98
        value: Swatch::light(135),  // #af5fff, L* 57
        fallback: Swatch::ansi_dark(Color::Magenta),
    },
];

/// The colour a spatial blast is painted in at 256 colours (`docs/UI.md` §7).
///
/// **`#ff5f00`, and orange is the one hue the material table never takes.** The twelve
/// pairs above follow Minecraft, which spends grey, brown, blue, red, green, cyan,
/// magenta, white and yellow — pure orange is the gap, and it reads as *fire* rather
/// than as ore, which is the whole of §7's *"not a material"*. `GoldOre`'s `#af5f00` is
/// the same hue three steps darker and is the nearest thing to it in the table.
///
/// **It cannot clear §4.2's own `ΔE ≥ 40` gate, and nothing could.** The palette
/// deliberately spans the hue circle and every lightness from `L* 15` to `L* 98`, so
/// wherever a twenty-fifth colour lands, some material is near it — white is 6 from
/// Quartz's `#eeeeee`, magenta is close to Crying Obsidian. The gate this answers to is
/// therefore `ΔE ≥ 25` (`the_blast_separates_from_every_material`), and the difference
/// is a difference in the question: §4.2's 40 measures *two static cells side by side*,
/// while a blast is a **region changing colour for 200 ms**, which is a far easier
/// discrimination. The glyph is what makes the lower gate safe — see [`blast`].
///
/// Chosen by eye against a running grid rather than by the script that filtered it,
/// which is what §7 asks for. It stays open to a deliberate retune; what is settled is
/// that changing it now fails a test.
pub const BLAST: Color = Color::Indexed(202);

/// The same, at 16 colours: **bright red**, ANSI 9.
///
/// The twelve mines already spend seven of the eight usable named colours — Black being
/// what a *hole* looks like — so the fallback column leaves no ordinary colour free. The
/// bright half of the palette does: `LightRed` is claimed by no mine, and its one
/// neighbour is the `Red` that Redstone and Quartz take. Named and not indexed, for the
/// reason the whole fallback column is named: at 16 colours the terminal's theme is
/// meant to be the authority.
pub const BLAST_ANSI16: Color = Color::LightRed;

/// The blast colour for `mode` — [`BLAST`] or [`BLAST_ANSI16`].
///
/// **One colour, and the glyph carries the shape.** §4.4's rule is that colour never
/// carries an answer alone, and a flash painted in colour alone would break it: on a
/// terminal that dropped the hue, or for a player who cannot separate orange from the
/// gold cell under it, the blast would be *invisible* rather than merely subtle. So the
/// widget spends the glyph channel too — a solid `█` on the first beat and a half-ink
/// `▒` on the second — which is also what lets this answer to a gentler contrast gate
/// than the material swatches do.
///
/// **One colour and not three.** §7 specifies *a single bright blast colour*: Explosive,
/// Jackhammer and Nuke differ in shape, and the shape is what the flash exists to show.
/// A colour per enchant would be a second vocabulary to learn for a fact the geometry
/// already states.
pub fn blast(mode: ColourMode) -> Color {
    match mode {
        ColourMode::Ansi256 => BLAST,
        ColourMode::Ansi16 => BLAST_ANSI16,
    }
}

/// Which of a mine's two blocks a cell holds.
///
/// A role and not a `Block`, because that is all the palette can answer: the
/// twelve rows are keyed by mine, and inside a mine there are exactly two
/// possibilities. Handing this function a `Block` would invite the caller to pass
/// one from a *different* mine, which has no colour here by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellRole {
    /// The bulk of a fresh grid — [`MineKind::common_block`].
    Common,
    /// The rarer block richness shifts weight toward — [`MineKind::value_block`].
    Value,
}

/// How many colours the terminal is being asked for.
///
/// A player preference, stored in the save (`docs/UI.md` §6.10) — which is why it is
/// a plain two-variant enum with a `Default` and no detection. What the *terminal*
/// reports is a separate question, printed beside this choice on the Settings screen
/// rather than allowed to override it: a player on a terminal that under-reports its
/// palette must still be able to ask for 256, and one who finds the swatches
/// indistinguishable must be able to ask for 16 on a terminal that offers more.
///
/// It carries serde derives because it is a [`Config`](crate::config::Config) field
/// and config travels inside the signed save. **The variant names are therefore
/// on-disk format** — `Ansi256` is that word in every save file — which is the same
/// rule [`SubTabKeys`](crate::config::SubTabKeys) states at length: what the player
/// reads comes from [`ColourMode::label`], so the display can be reworded freely and
/// only a rename of the variant is a format change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColourMode {
    /// Every block gets its own swatch — the twenty-four of [`PALETTE`].
    #[default]
    Ansi256,
    /// One colour per mine; the stipple carries common-versus-value alone.
    Ansi16,
}

impl ColourMode {
    /// The value column's text on the Settings screen (`docs/UI.md` §6.10).
    ///
    /// Bare numerals rather than `256 colours`, because the row is already labelled
    /// `Colour` and the wireframe's column is eleven characters wide.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ansi256 => "256",
            Self::Ansi16 => "16",
        }
    }
}

/// The row describing `kind`.
///
/// The index comes from an exhaustive `match`, so it is always one of `0..12` and
/// [`PALETTE`] always has twelve rows — the type system pins the length. That the
/// row at index `i` really is the mine the `match` sent there is not something the
/// compiler can check, so the `the_table_order_matches_the_lookup` test does.
pub fn palette(kind: MineKind) -> MinePalette {
    let index = match kind {
        MineKind::Stone => 0,
        MineKind::Coal => 1,
        MineKind::Iron => 2,
        MineKind::Gold => 3,
        MineKind::Lapis => 4,
        MineKind::Redstone => 5,
        MineKind::Emerald => 6,
        MineKind::Diamond => 7,
        MineKind::Quartz => 8,
        MineKind::AncientDebris => 9,
        MineKind::Obsidian => 10,
        MineKind::Amethyst => 11,
    };
    PALETTE[index]
}

/// The swatch one cell should be painted in.
///
/// The `Ansi16` arm ignores the role entirely, and that *is* the fallback: both
/// roles share the mine's one colour, and the value cell's stipple — unconditional
/// in both modes — is what still separates them. One rendering model with a channel
/// switched off, not a second code path.
pub fn swatch(kind: MineKind, role: CellRole, mode: ColourMode) -> Swatch {
    let entry = palette(kind);
    match (mode, role) {
        (ColourMode::Ansi256, CellRole::Common) => entry.common,
        (ColourMode::Ansi256, CellRole::Value) => entry.value,
        (ColourMode::Ansi16, _) => entry.fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A colour in CIELAB — the space the palette's gate is quoted in.
    ///
    /// Only `l` is ever read on its own (lightness is the channel the gate leans
    /// on), but `a` and `b` are needed for `ΔE`, so the three travel together.
    #[derive(Clone, Copy, Debug)]
    struct Lab {
        l: f64,
        a: f64,
        b: f64,
    }

    /// True black and true white, as the ink constants name them.
    const BLACK: (u8, u8, u8) = (0, 0, 0);
    const WHITE: (u8, u8, u8) = (255, 255, 255);

    /// Nominal sRGB for the eight ANSI colours the fallback column uses.
    ///
    /// **Nominal is the caveat**: a terminal may remap any of these, and the
    /// fallback exists precisely so that it can. What the figures below can still
    /// prove is that the *choice of ink* is right for a default terminal, which is
    /// the only version of the question that has an answer.
    fn ansi_rgb(colour: Color) -> (u8, u8, u8) {
        match colour {
            Color::Red => (0xcd, 0x00, 0x00),
            Color::Green => (0x00, 0xcd, 0x00),
            Color::Yellow => (0xcd, 0xcd, 0x00),
            Color::Blue => (0x00, 0x00, 0xee),
            Color::Magenta => (0xcd, 0x00, 0xcd),
            Color::Cyan => (0x00, 0xcd, 0xcd),
            Color::White => WHITE,
            Color::Black => BLACK,
            // `unreachable` and not `panic`: the fallback column below names only
            // the seven colours above, so reaching here means the table grew a
            // colour without telling this helper — a fact about the code, not a
            // runtime failure. The workspace warns on `panic!` for that reason.
            other => unreachable!("no nominal sRGB recorded for {other:?}"),
        }
    }

    /// sRGB for a 256-colour index, over the two ranges the palette actually uses:
    /// the 6×6×6 colour cube (16..232) and the grey ramp (232..256).
    ///
    /// The first sixteen indices are deliberately unhandled — they are the ANSI
    /// colours, which [`ansi_rgb`] answers, and which the 256-colour column never
    /// names because a themed terminal would move them.
    fn indexed_rgb(index: u8) -> (u8, u8, u8) {
        const CUBE: [u8; 6] = [0, 0x5f, 0x87, 0xaf, 0xd7, 0xff];
        match index {
            16..=231 => {
                let n = u32::from(index) - 16;
                (
                    CUBE[(n / 36) as usize],
                    CUBE[(n / 6 % 6) as usize],
                    CUBE[(n % 6) as usize],
                )
            }
            232..=255 => {
                let grey = 8 + (u32::from(index) - 232) * 10;
                let grey = grey as u8;
                (grey, grey, grey)
            }
            other => unreachable!("index {other} is an ANSI colour, not a palette entry"),
        }
    }

    /// Whatever `colour` is, as sRGB.
    fn rgb(colour: Color) -> (u8, u8, u8) {
        match colour {
            Color::Indexed(index) => indexed_rgb(index),
            named => ansi_rgb(named),
        }
    }

    /// sRGB to CIELAB, D65 — the standard conversion, written out because pulling
    /// a colour-science crate in for one test would put a dependency in the
    /// manifest that ships in no build.
    fn lab(colour: Color) -> Lab {
        fn linearise(channel: u8) -> f64 {
            let c = f64::from(channel) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        fn f(t: f64) -> f64 {
            if t > 216.0 / 24389.0 {
                t.cbrt()
            } else {
                (841.0 / 108.0) * t + 4.0 / 29.0
            }
        }

        let (r, g, b) = rgb(colour);
        let (r, g, b) = (linearise(r), linearise(g), linearise(b));

        // sRGB to XYZ, then normalised by the D65 white point.
        let x = (r * 0.412_456_4 + g * 0.357_576_1 + b * 0.180_437_5) / 0.950_47;
        let y = r * 0.212_672_9 + g * 0.715_152_2 + b * 0.072_175_0;
        let z = (r * 0.019_333_9 + g * 0.119_192_0 + b * 0.950_304_1) / 1.088_83;
        let (fx, fy, fz) = (f(x), f(y), f(z));

        Lab {
            l: 116.0 * fy - 16.0,
            a: 500.0 * (fx - fy),
            b: 200.0 * (fy - fz),
        }
    }

    /// CIE76 colour difference: the plain Euclidean distance in Lab.
    fn delta_e(one: Lab, other: Lab) -> f64 {
        ((one.l - other.l).powi(2) + (one.a - other.a).powi(2) + (one.b - other.b).powi(2)).sqrt()
    }

    #[test]
    fn every_pair_clears_the_contrast_gate() {
        for row in PALETTE {
            let (common, value) = (lab(row.common.bg), lab(row.value.bg));

            let difference = delta_e(common, value);
            assert!(
                difference >= 40.0,
                "{:?}: the pair measures ΔE {difference:.1}, under the gate of 40",
                row.mine
            );

            // Lightness carries the pair on its own, so the distinction survives a
            // colour-vision deficiency: none of them flattens L*.
            let lightness = (common.l - value.l).abs();
            assert!(
                lightness >= 20.0,
                "{:?}: the pair differs by ΔL* {lightness:.1}, under the gate of 20",
                row.mine
            );
        }
    }

    #[test]
    fn no_swatch_is_dark_enough_to_read_as_a_hole() {
        for row in PALETTE {
            for (role, swatch) in [("common", row.common), ("value", row.value)] {
                let lightness = lab(swatch.bg).l;
                assert!(
                    lightness >= 12.0,
                    "{:?}'s {role} swatch sits at L* {lightness:.1}, where a broken cell is",
                    row.mine
                );
            }
        }
    }

    #[test]
    fn every_swatch_can_be_written_on() {
        // The stipple and the crack are glyphs drawn *on* the swatch, so an ink
        // that matches its background erases the one distinction the mine screen
        // exists for. 40 is the same gate the pairs answer to, which is what makes
        // this a measurement rather than a preference.
        for row in PALETTE {
            for (role, swatch) in [
                ("common", row.common),
                ("value", row.value),
                ("fallback", row.fallback),
            ] {
                let contrast = (lab(swatch.bg).l - lab(swatch.ink).l).abs();
                assert!(
                    contrast >= 40.0,
                    "{:?}'s {role} ink is only ΔL* {contrast:.1} from its swatch",
                    row.mine
                );
            }
        }
    }

    #[test]
    fn the_table_order_matches_the_lookup() {
        // The `match` in `palette` pins that every mine *has* a row; nothing pins
        // that it is the right one. Walking the table itself — rather than a second
        // list of the twelve mines — is what keeps this test from being a copy of
        // the core's own inventory.
        for (index, row) in PALETTE.iter().enumerate() {
            assert_eq!(
                palette(row.mine),
                *row,
                "row {index} describes {:?} but the lookup sends it elsewhere",
                row.mine
            );
        }
    }

    #[test]
    fn the_sixteen_colour_mode_gives_a_mine_one_colour() {
        for row in PALETTE {
            let common = swatch(row.mine, CellRole::Common, ColourMode::Ansi16);
            let value = swatch(row.mine, CellRole::Value, ColourMode::Ansi16);
            assert_eq!(common, value, "{:?} kept two colours at 16", row.mine);
            assert_ne!(
                common.bg,
                Color::Black,
                "{:?} is painted in what a broken cell looks like",
                row.mine
            );
        }
    }

    #[test]
    fn the_two_hundred_and_fifty_six_colour_mode_separates_the_roles() {
        for row in PALETTE {
            let common = swatch(row.mine, CellRole::Common, ColourMode::Ansi256);
            let value = swatch(row.mine, CellRole::Value, ColourMode::Ansi256);
            assert_ne!(common.bg, value.bg, "{:?} drew one colour twice", row.mine);
        }
    }

    /// The blast reads as "not a material" — measured, against all twenty-four.
    ///
    /// **The gate is 25 and not §4.2's 40, and the number is the argument.** Nothing
    /// could clear 40 here: the twelve pairs span the hue circle and every lightness
    /// from 15 to 98, so a twenty-fifth colour has neighbours wherever it lands. What
    /// separates this question from that one is that §4.2 asks whether two cells drawn
    /// *at the same time, next to each other* can be told apart, while this asks whether
    /// a region that just **changed colour for 200 ms** reads as a change — a far easier
    /// discrimination, and one the `█`/`▒` glyphs answer even where the hue does not.
    ///
    /// It walks the table rather than a list retyped here, so a re-palette that moved a
    /// material next to the blast fails this instead of shipping an invisible flash.
    #[test]
    fn the_blast_separates_from_every_material() {
        let blast = lab(BLAST);
        for row in PALETTE {
            for (role, swatch) in [("common", row.common), ("value", row.value)] {
                let difference = delta_e(blast, lab(swatch.bg));
                assert!(
                    difference >= 25.0,
                    "the blast is only ΔE {difference:.1} from {:?}'s {role} swatch",
                    row.mine
                );
            }
        }
    }

    /// At 16 colours the blast has to be a colour no mine took, and not the one a hole
    /// is drawn as.
    ///
    /// The seven fallbacks spend nearly the whole ordinary palette, so this is the
    /// assertion that would fail first if a thirteenth mine were added — which is the
    /// point of walking the table instead of asserting `LightRed != Red` and calling it
    /// done.
    #[test]
    fn the_sixteen_colour_blast_is_a_colour_no_mine_claims() {
        assert_ne!(
            BLAST_ANSI16,
            Color::Black,
            "the blast is painted in what a broken cell looks like"
        );
        for row in PALETTE {
            assert_ne!(
                BLAST_ANSI16, row.fallback.bg,
                "{:?} is painted in the blast colour at 16",
                row.mine
            );
        }
    }

    #[test]
    fn each_mode_gets_its_own_blast_colour() {
        assert_eq!(blast(ColourMode::Ansi256), BLAST);
        assert_eq!(blast(ColourMode::Ansi16), BLAST_ANSI16);
        // The two modes must not answer alike: an `Indexed` at 16 colours is exactly the
        // pinning the fallback exists to undo.
        assert_ne!(blast(ColourMode::Ansi256), blast(ColourMode::Ansi16));
    }

    #[test]
    fn the_default_mode_is_the_richer_one() {
        assert_eq!(ColourMode::default(), ColourMode::Ansi256);
    }

    #[test]
    fn each_constructor_pairs_its_swatch_with_the_ink_its_name_promises() {
        // The four constructors exist so the ink decision is readable off every row
        // of `PALETTE` — `light` or `dark` in the table, and the reader knows what
        // will be written on it. That only holds if the name and the ink agree, and
        // nothing above checks it: `every_swatch_can_be_written_on` measures the
        // *contrast* of whatever pair ended up in the table, so a `light` that handed
        // out light ink would be caught there only if the result happened to be
        // illegible. Here the promise itself is the assertion.
        //
        // These are `const fn`, and the table is built at compile time, so calling
        // them at runtime is also the only way they are ever executed at all.
        assert_eq!(
            Swatch::light(30),
            Swatch {
                bg: Color::Indexed(30),
                ink: INK_ON_LIGHT
            }
        );
        assert_eq!(
            Swatch::dark(235),
            Swatch {
                bg: Color::Indexed(235),
                ink: INK_ON_DARK
            }
        );

        // The ANSI pair takes a named colour rather than an index — the fallback's
        // whole point is to let the terminal's theme remap it — and its ink is named
        // too, so it travels with the background it was chosen against.
        assert_eq!(
            Swatch::ansi_light(Color::Yellow),
            Swatch {
                bg: Color::Yellow,
                ink: Color::Black
            }
        );
        assert_eq!(
            Swatch::ansi_dark(Color::Blue),
            Swatch {
                bg: Color::Blue,
                ink: Color::White
            }
        );
    }
}
