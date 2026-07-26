//! Text formatting shared across screens.
//!
//! Two helpers the screens lean on. [`grouped`] enforces the cross-cutting rule
//! that numbers are **exact, with a thousands separator, and never abbreviated**
//! (UI.md §5.6): `1 240`, `418 297`, never `1.2k`. [`justified`] lays a label and
//! a value on one row with the value flush to the right — the shape a roadmap row,
//! a stat line and a table row all repeat. Keeping both here means the grouping
//! and the alignment cannot drift between the XP gauge and the inventory table.

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
}
