//! Text formatting shared across screens.
//!
//! Three helpers the screens lean on. [`grouped`] enforces the cross-cutting rule
//! that numbers are **exact, with a thousands separator, and never abbreviated**
//! (UI.md §5.6): `1 240`, `418 297`, never `1.2k`. [`justified`] lays a label and
//! a value on one row with the value flush to the right — the shape a roadmap row,
//! a stat line and a table row all repeat. [`xp_progress`] is the one reading that
//! has a *non-numeric* answer, and it is here so all three screens that print it
//! give the same one. Keeping them together means the grouping and the alignment
//! cannot drift between the XP gauge and the inventory table.

/// Groups `n` into space-separated thousands: `1240` becomes `"1 240"`.
///
/// A **plain space**, not a comma or a dot, because the wireframes are drawn that
/// way (`1 240 / 2 300`) and because the two punctuation choices are the two that
/// collide with a decimal point in the locales this reader lives between. The
/// separator is the ASCII space so the rendered row matches the frame byte for
/// byte, which is what the layout tests assert against.
pub fn grouped(n: u32) -> String {
    // Built from the least-significant digit up — grouping counts from the right,
    // so the string is assembled reversed and flipped once at the end rather than
    // repeatedly measuring how far the current digit sits from the decimal point.
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (count, ch) in digits.chars().rev().enumerate() {
        // A separator every third digit, but never a leading one: `count == 0` is
        // the first digit and `count % 3 == 0` there would prepend a stray space.
        if count > 0 && count % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

/// A line of exactly `width` columns with `left` at the start and `right` flush to
/// the end — the shape a roadmap row, a stat line and a table row all repeat.
///
/// Counts columns in `chars`, not bytes: marks like `▸●` and the `·` separator are
/// multi-byte but one column wide, and byte lengths would over-pad every row that
/// carries one. If `left` and `right` together already exceed `width`, the pad is
/// zero rather than negative — the row overflows visibly instead of panicking on a
/// subtraction the way a `usize` underflow would.
pub fn justified(left: &str, right: &str, width: usize) -> String {
    let used = left.chars().count() + right.chars().count();
    let pad = width.saturating_sub(used);
    let mut out = String::with_capacity(left.len() + pad + right.len());
    out.push_str(left);
    for _ in 0..pad {
        out.push(' ');
    }
    out.push_str(right);
    out
}

/// The XP readout: `1 240 / 2 300`, or [`MAXED`] once there is no next level.
///
/// `to_next` is an [`Option`] because
/// [`Player::experience_to_next_level`](skylode_core::player::Player::experience_to_next_level)
/// is: at [`LEVEL_CAP`](skylode_core::tunables::LEVEL_CAP) there is no rung left to
/// price, and the core says so in the type rather than with a `0` sentinel. Written
/// once here because **three screens print this same reading** — the Mine gauge,
/// the Stats Progression panel and the Levels title — and three independent
/// `map_or`s would be three chances to word the capped case differently, or to
/// divide by the zero a sentinel would have handed them.
pub fn xp_progress(xp: u32, to_next: Option<u32>) -> String {
    match to_next {
        Some(needed) => format!("{} / {}", grouped(xp), grouped(needed)),
        None => MAXED.to_owned(),
    }
}

/// What a track with nothing left to sell reads.
///
/// The same word the Upgrades Mines rows already use for a maxed size or richness
/// track, reused rather than reinvented: a player who has seen `Stone  Size
/// maxed` should read `Lv 50  maxed` as the same statement about a different
/// ladder.
pub const MAXED: &str = "maxed";

/// The gauge fill for that same reading, in `0.0..=1.0`.
///
/// **Full, not empty, at the cap.** A capped player has earned every level there
/// is, so an empty bar would read as "no progress" for the one state that is total
/// progress. Paired with [`xp_progress`] here so the number and the bar cannot
/// disagree about what the cap looks like.
pub fn xp_ratio(xp: u32, to_next: Option<u32>) -> f64 {
    to_next.map_or(1.0, |needed| f64::from(xp) / f64::from(needed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_below_a_thousand_is_left_untouched() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(9), "9");
        assert_eq!(grouped(680), "680");
    }

    #[test]
    fn a_thousand_gets_one_separator() {
        assert_eq!(grouped(1_240), "1 240");
        assert_eq!(grouped(2_300), "2 300");
    }

    #[test]
    fn separators_repeat_every_three_digits() {
        assert_eq!(grouped(418_297), "418 297");
        assert_eq!(grouped(1_000_000), "1 000 000");
    }

    #[test]
    fn the_boundary_at_exactly_a_thousand_groups_correctly() {
        // 1000 is the first value that groups: the separator sits after the 1, not
        // before it, which is the off-by-one `count > 0` guards against.
        assert_eq!(grouped(1_000), "1 000");
    }

    #[test]
    fn justified_puts_the_value_flush_right_and_fills_the_gap() {
        assert_eq!(justified("Lv", "50", 10), "Lv      50");
        assert_eq!(justified("XP", "1 240", 10), "XP   1 240");
    }

    #[test]
    fn justified_measures_multibyte_marks_as_one_column() {
        // `▸●` is four bytes but two columns: a byte count would pad two short.
        let line = justified("▸●", "x", 6);
        assert_eq!(line.chars().count(), 6, "padded by bytes, not columns");
        assert!(line.starts_with("▸●"));
        assert!(line.ends_with('x'));
    }

    #[test]
    fn justified_overflows_rather_than_panicking_when_too_narrow() {
        // Left and right already exceed the width: the pad floors at zero, and the
        // two simply abut instead of underflowing the `usize` subtraction.
        assert_eq!(justified("abcd", "efgh", 6), "abcdefgh");
    }

    #[test]
    fn xp_below_the_cap_reads_as_a_grouped_fraction() {
        assert_eq!(xp_progress(1_240, Some(2_300)), "1 240 / 2 300");
        assert_eq!(xp_ratio(1_150, Some(2_300)), 0.5);
    }

    #[test]
    fn xp_at_the_cap_reads_as_maxed_on_a_full_bar() {
        // The pairing is the point: a capped player has earned every level there
        // is, so the word and the bar must both say *finished* rather than the
        // word saying finished over an empty bar.
        assert_eq!(xp_progress(4_900, None), MAXED);
        assert_eq!(xp_ratio(4_900, None), 1.0);
    }
}
