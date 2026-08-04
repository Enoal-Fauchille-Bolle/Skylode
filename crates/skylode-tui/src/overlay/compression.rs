//! The compression dialog (UI.md §6.6).
//!
//! A bounded spinner (`◄ 12 ►`), not a typed number, because the amount has a known
//! maximum and a spinner cannot be wrong. The inverse — decompress — is **the same
//! frame with the arithmetic reversed**, which is free-and-lossless-both-ways showing
//! up as a UI economy rather than a second screen: same five lines, same height, each
//! number read in the other denomination.
//!
//! That mirror is a recorded departure (`docs/UI.md` §5.3.1) only in the sense that
//! no wireframe ever drew it — §6.6 specified the inverse in a sentence and left the
//! frame to be derived. Nothing here is invented beyond reading the counted frame in
//! the other direction: `Costs` is still what the operation spends and `Leaves` still
//! what remains of the pile it spends from, and what the operation *yields* is still
//! the number in the spinner.

use ratatui::{Frame, layout::Rect};
use skylode_core::{
    inventory::Inventory,
    material::{Item, Material},
    tunables::RAW_PER_COMPRESSED,
};

use crate::{format::grouped, overlay::Conversion};

/// How many units of `material` a conversion in `direction` could convert at most.
///
/// **The one place the ceiling is computed**, read by the dialog to draw its `all
/// (13)` and by [`App`](crate::app::App) to clamp the spinner and to decide whether
/// there is anything to open a dialog *for*. It is display arithmetic and not a rule
/// — the Compress panel already prints `Compressible now` from the same division —
/// so answering it here costs the core nothing and leaves no second copy of a rule in
/// the front-end.
///
/// A raw pile converts in whole hundreds, so the compress bound floors; a Compressed
/// pile converts one for one, so the decompress bound is the count itself.
pub fn max_units(inventory: &Inventory, material: Material, direction: Conversion) -> u32 {
    match direction {
        Conversion::Compress => inventory.count(Item::Raw(material)) / RAW_PER_COMPRESSED,
        Conversion::Decompress => inventory.count(Item::Compressed(material)),
    }
}

/// The column the bare `Raw held` figure is right-aligned to, from the counted frame.
const HELD_COLUMN: usize = 21;

/// The column the `Costs` / `Leaves` figures end at — three earlier, because a
/// denomination word follows them on the same line.
const AMOUNT_COLUMN: usize = 18;

/// One `label … figure` line, the figure right-aligned to end at `column`.
///
/// The padding is computed from the label rather than written out, which is what lets
/// the two directions share a layout: `Raw held` and `Compressed held` are seven
/// characters apart, and hand-spacing the frame would mean two frames that only look
/// alike. `saturating_sub` because a label longer than the column has no room to pad
/// into and should push the figure along rather than underflow.
fn aligned(label: &str, figure: &str, column: usize) -> String {
    let width = column.saturating_sub(label.len());
    format!(" {label}{figure:>width$}")
}

/// Draws the dialog for `units` of `material`, in whichever direction.
///
/// Takes the [`Inventory`] rather than the four numbers it needs, because every one
/// of them is a reading of the same pile and passing them separately is four chances
/// to hand the frame a set that does not add up. The whole box is derived here from
/// `(what is held, how many units)`.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    inventory: &Inventory,
    material: Material,
    direction: Conversion,
    units: u32,
) {
    let name = material.name();
    let max = max_units(inventory, material, direction);

    // Everything the frame says, read in the denomination the direction spends from.
    // Naming the five once here is what makes the two boxes one frame read twice
    // rather than two layouts that happen to resemble each other.
    let (verb, held_label, held, spent, denomination) = match direction {
        Conversion::Compress => (
            "Compress",
            "Raw held",
            inventory.count(Item::Raw(material)),
            units.saturating_mul(RAW_PER_COMPRESSED),
            "raw",
        ),
        Conversion::Decompress => (
            "Decompress",
            "Compressed held",
            inventory.count(Item::Compressed(material)),
            units,
            "Compressed",
        ),
    };
    let amount = |label: &str, value: u32| {
        format!(
            "{} {denomination}",
            aligned(label, &grouped(value), AMOUNT_COLUMN)
        )
    };

    super::modal_with_hint(
        frame,
        area,
        48,
        11,
        &format!(" {verb} {name} "),
        &[
            "",
            &aligned(held_label, &grouped(held), HELD_COLUMN),
            "",
            // `{units:^4}` centres the count between the arrows, so the spinner does
            // not shuffle sideways as it crosses ten and a hundred.
            &format!(" {verb:<11}◄ {units:^4} ►"),
            &amount("Costs", spent),
            &amount("Leaves", held.saturating_sub(spent)),
            "",
        ],
        // The height is fixed at 11, which already counted this row when it was the
        // last entry above — so moving it out changes its colour and nothing else.
        Some(&format!(" a  all ({max})   Enter  do it")),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A purse holding one material in both denominations.
    fn holding(compressed: u32, raw: u32) -> Inventory {
        let mut inventory = Inventory::new();
        if compressed > 0 {
            inventory.add(Item::Compressed(Material::Iron), compressed);
        }
        if raw > 0 {
            inventory.add(Item::Raw(Material::Iron), raw);
        }
        inventory
    }

    fn draw(inventory: &Inventory, direction: Conversion, units: u32) -> String {
        crate::overlay::render_to_string(|frame, area| {
            render(frame, area, inventory, Material::Iron, direction, units);
        })
    }

    /// The counted frame of §6.6, reproduced from a purse rather than transcribed:
    /// 1 350 raw, twelve units on the spinner, thirteen available.
    #[test]
    fn the_compress_frame_matches_the_counted_one() {
        let frame = draw(&holding(0, 1_350), Conversion::Compress, 12);

        assert!(frame.contains("Compress Iron"), "{frame}");
        assert!(frame.contains("Raw held        1 350"), "{frame}");
        assert!(frame.contains("◄  12  ►"), "{frame}");
        assert!(frame.contains("Costs        1 200 raw"), "{frame}");
        assert!(frame.contains("Leaves         150 raw"), "{frame}");
        assert!(frame.contains("a  all (13)"), "{frame}");
    }

    /// The inverse is the same frame with every number read in the other
    /// denomination — the whole claim §6.6 makes about it in one sentence.
    #[test]
    fn the_decompress_frame_is_the_same_one_read_backwards() {
        let frame = draw(&holding(13, 0), Conversion::Decompress, 2);

        assert!(frame.contains("Decompress Iron"), "{frame}");
        assert!(frame.contains("Compressed held    13"), "{frame}");
        assert!(frame.contains("◄  2   ►"), "{frame}");
        assert!(frame.contains("Costs            2 Compressed"), "{frame}");
        assert!(frame.contains("Leaves          11 Compressed"), "{frame}");
        assert!(frame.contains("a  all (13)"), "{frame}");
    }

    /// The two bounds, which are not the same arithmetic: raw converts in whole
    /// hundreds and floors, Compressed converts one for one.
    ///
    /// The remainder is the interesting half — 1 350 raw offers thirteen units and
    /// not thirteen and a half, and the fifty left over is what `Leaves` reports.
    #[test]
    fn the_ceiling_floors_one_way_and_counts_the_other() {
        let purse = holding(13, 1_350);
        assert_eq!(
            max_units(&purse, Material::Iron, Conversion::Compress),
            13,
            "the raw bound did not floor"
        );
        assert_eq!(
            max_units(&purse, Material::Iron, Conversion::Decompress),
            13
        );

        // Below one whole unit there is nothing to convert, which is the case `App`
        // reads to toast instead of opening a dialog that could only be cancelled.
        let short = holding(0, 99);
        assert_eq!(max_units(&short, Material::Iron, Conversion::Compress), 0);
        assert_eq!(max_units(&short, Material::Iron, Conversion::Decompress), 0);
    }

    /// The frame is about the pile under the cursor, so the material's own name is on
    /// the box — including the two-word ones, which is what the fixed columns have to
    /// survive.
    #[test]
    fn the_dialog_is_titled_with_the_pile_it_converts() {
        let mut inventory = Inventory::new();
        inventory.add(Item::Raw(Material::AncientDebris), 500);
        let frame = crate::overlay::render_to_string(|frame, area| {
            render(
                frame,
                area,
                &inventory,
                Material::AncientDebris,
                Conversion::Compress,
                5,
            );
        });

        assert!(frame.contains("Compress Ancient Debris"), "{frame}");
        assert!(frame.contains("Raw held          500"), "{frame}");
    }
}
