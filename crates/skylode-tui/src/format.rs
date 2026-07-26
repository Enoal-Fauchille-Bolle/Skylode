//! Number formatting shared across screens.
//!
//! The design's cross-cutting rule is that numbers are **exact, with a thousands
//! separator, and never abbreviated** (UI.md §5.6, the Levels and Stats frames):
//! `1 240`, `418 297`, never `1.2k`. An idle game earns numbers a player wants to
//! read precisely, so the whole UI groups digits the same way — one function, so
//! the grouping cannot drift between the XP gauge and the inventory table.

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
}
