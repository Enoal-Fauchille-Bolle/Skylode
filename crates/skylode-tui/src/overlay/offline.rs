//! The offline-summary modal (UI.md §6.4).
//!
//! Shown once on resume, and it **states the multiplication** — `rate × elapsed` is
//! the whole offline mechanic, so printing it makes the number checkable in the
//! player's head. The parenthesised denomination split is the same raw total shown
//! two ways, not a compression.
//!
//! **When it appears is not this render's business, and it is not a threshold
//! either.** [`GameState::resume`] answers [`None`] on a backward clock and on a span
//! of zero, and the session shows this only when the report actually *paid* something
//! — which falls out of [`gained`](OfflineReport::gained) being non-empty, since the
//! auto-miner credits whole blocks and a few seconds of absence complete none. A
//! `Welcome back, +0` after a daylight-saving change is a support ticket about a bug
//! that is not one.
//!
//! [`GameState::resume`]: skylode_core::game::GameState::resume

use ratatui::{Frame, layout::Rect};
use skylode_core::{
    game::OfflineReport,
    material::Item,
    tunables::{AUTO_MINER_MILLIBLOCKS_PER_TICK, MILLIBLOCKS_PER_BLOCK, TICKS_PER_SECOND},
};

use crate::{
    config::NumberFormat,
    format::{denominations, grouped, grouped_u64, span},
};

/// How wide the frame is drawn.
const WIDTH: u16 = 60;

/// The width the material names are padded to, so the parenthesised split lines up.
const NAME_COLUMN: usize = 16;

/// The auto-miner's rate in blocks per second, as the summary prints it.
///
/// **Derived from the tunable rather than from the report**, which is what makes the
/// printed multiplication a real check instead of a circular one: `blocks` is what
/// this rate produced over `counted`, so a player who multiplies the two and gets the
/// third number has verified the mechanic. Dividing the report's own blocks by its own
/// span would print a number that agrees with itself no matter what the rules did.
fn rate() -> f64 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a rate of a few blocks a second, printed to two decimals"
    )]
    let per_second = (AUTO_MINER_MILLIBLOCKS_PER_TICK * TICKS_PER_SECOND) as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a compile-time constant of one thousand"
    )]
    let per_block = MILLIBLOCKS_PER_BLOCK as f64;
    per_second / per_block
}

/// Draws the offline summary for `report`.
pub fn render(frame: &mut Frame, area: Rect, report: &OfflineReport, format: NumberFormat) {
    let mut lines = vec![
        String::new(),
        away(report),
        " The auto-miner kept going.".to_owned(),
    ];

    lines.push(String::new());
    // The amounts are right-aligned against the widest of *this* report rather than a
    // number written down here: the totals run from tens to millions over a run, and a
    // fixed column would either waste half the frame early or stop lining up late.
    let width = report
        .gained
        .iter()
        .map(|&(_, amount)| grouped(amount, format).chars().count())
        .max()
        .unwrap_or(0);
    lines.extend(
        report
            .gained
            .iter()
            .map(|&(item, amount)| gain(item, amount, width, format)),
    );

    lines.push(String::new());
    // The mechanic, spelled out. `counted` and not `elapsed`, so a capped absence
    // multiplies out to the number above it rather than to one the cap already cut.
    lines.push(format!(
        " Rate  {:.2} blocks/s  ×  {}  =  {} blocks",
        rate(),
        span(report.counted),
        grouped_u64(report.blocks, format)
    ));
    lines.push(String::new());

    // Two for the borders and one for the hint, which is no longer in `lines` — it is
    // chrome and takes the muted hue, so it goes to `modal_with_hint` instead.
    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(3);
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    super::modal_with_hint(
        frame,
        area,
        WIDTH,
        height,
        " Welcome back ",
        &borrowed,
        Some(" Enter  collect"),
    );
}

/// How long the player was away, and what of it was paid for.
///
/// **The cap is stated rather than applied silently**, which is why
/// [`OfflineReport`] carries both durations: *"away for 9d 4h — counted 7d"* is
/// honest, and quietly paying for seven of nine reads as a bug the day someone
/// checks.
fn away(report: &OfflineReport) -> String {
    if report.capped {
        return format!(
            " You were away for  {}  —  counted {}",
            span(report.elapsed),
            span(report.counted)
        );
    }
    format!(" You were away for  {}", span(report.elapsed))
}

/// One credited line: `+12 480  Iron            (124 Compressed + 80)`.
///
/// The parenthesis appears only when there is a second reading to give. Under one
/// Compressed unit [`denominations`] answers with the same figure already printed, and
/// a line that says `+80  Coal            (80)` is a column spent on nothing.
fn gain(item: Item, amount: u32, width: usize, format: NumberFormat) -> String {
    let split = denominations(amount, format);
    let name = item.material().name();
    // The `+` sits flush against the column and the padding goes *inside* it, so a
    // stack of gains reads as one number per row rather than as a `+` adrift from its
    // own figure.
    let total = format!(" +{:>width$}  ", grouped(amount, format));
    if split == grouped(amount, format) {
        return format!("{total}{name}");
    }
    format!("{total}{name:<NAME_COLUMN$}({split})")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use skylode_core::material::Material;

    use super::*;

    fn a_report() -> OfflineReport {
        OfflineReport {
            elapsed: Duration::from_secs(6 * 3_600 + 12 * 60),
            counted: Duration::from_secs(6 * 3_600 + 12 * 60),
            capped: false,
            blocks: 4_910,
            gained: vec![
                (Item::Raw(Material::Iron), 12_480),
                (Item::Raw(Material::Coal), 372),
                (Item::Raw(Material::Gold), 31),
            ],
        }
    }

    fn drawn(report: &OfflineReport) -> String {
        crate::overlay::render_to_string(|frame, area| {
            render(frame, area, report, NumberFormat::default());
        })
    }

    #[test]
    fn it_shows_the_span_the_gains_and_the_multiplication() {
        let frame = drawn(&a_report());
        assert!(frame.contains("Welcome back"), "{frame}");
        assert!(frame.contains("You were away for  6h 12m"), "{frame}");
        assert!(frame.contains("+12 480  Iron"), "{frame}");
        // Right-aligned against the widest of this report, so the digits stack.
        assert!(frame.contains("+   372  Coal"), "{frame}");
        assert!(frame.contains("+    31  Gold"), "{frame}");
        // The mechanic, spelled out: rate × elapsed, and the product it landed on.
        assert!(
            frame.contains("Rate  0.22 blocks/s  ×  6h 12m  =  4 910 blocks"),
            "{frame}"
        );
        assert!(frame.contains("Enter  collect"), "{frame}");
    }

    #[test]
    fn a_total_worth_compressing_is_shown_both_ways() {
        let frame = drawn(&a_report());
        assert!(frame.contains("(124 Compressed + 80)"), "{frame}");
        // And a total under one Compressed unit keeps its one column: `(31)` would be
        // the same figure twice.
        assert!(!frame.contains("(31)"), "{frame}");
    }

    #[test]
    fn a_capped_absence_says_how_much_of_it_was_paid_for() {
        let mut report = a_report();
        report.elapsed = Duration::from_secs(9 * 86_400 + 4 * 3_600);
        report.counted = Duration::from_secs(7 * 86_400);
        report.capped = true;

        let frame = drawn(&report);
        assert!(
            frame.contains("away for  9d 4h  —  counted 7d"),
            "the cap was applied in silence: {frame}"
        );
        // And the multiplication uses what was *counted*, or it would not reach the
        // total printed above it.
        assert!(frame.contains("×  7d "), "{frame}");
    }
}
