//! The chrome's colours: borders, titles, gauges, and the semantic marks.
//!
//! This is the **second** palette in the crate, and it is deliberately not part of
//! the first. [`palette`](crate::palette) answers *"what colour is iron?"* — a
//! question about a material, with one right answer, measurable against a gate. This
//! module answers *"what does a refusal look like?"* — a question about the
//! interface, whose right answer depends on a terminal neither module can see. Two
//! questions, two colour spaces, two kinds of test; folding them into one table
//! would leave a module whose rustdoc has to argue both cases at once.
//!
//! ## Why named colours here, and indexed ones there
//!
//! Every constant below is a **named ANSI** colour, which is exactly what
//! [`palette`](crate::palette) refuses for its 256-colour column — and the reversal
//! is the point. There, a remapped colour is a *hazard*: `Color::Black` on a
//! Solarized terminal is a dark teal, and the contrast figures were computed against
//! true black, so the palette names `Color::Indexed(16)` to pin what the player
//! actually sees. Here, a remapped colour is the *feature*: chrome sits on the
//! terminal's own background — the same background a broken cell is drawn as — so
//! the theme is the authority on what "grey" should be, and a hardcoded index is the
//! thing that could turn out illegible on a light terminal.
//!
//! Two consequences fall out for free. The chrome needs **no 16-colour fallback**,
//! because it never asked for more than sixteen: [`ColourMode`] stays a question
//! about the grid alone. And these are the colours ratatui's own [`Stylize`] trait
//! exposes (`.cyan()`, `.dark_gray()`), so call sites style with the crate's idiom
//! rather than dropping out of it into `Color::Indexed`.
//!
//! ## Why there is no contrast gate
//!
//! [`palette`](crate::palette) asserts `ΔE ≥ 40` in CIELAB because it knows both
//! colours it is comparing: a swatch and the ink written on it. This module knows
//! only one. The other is the player's background, which is not merely unknown but
//! *unknowable* from inside the process — so a test measuring [`AFFORDABLE`] against
//! nominal black would be proving something about a terminal the player may not be
//! using, and would pass just as happily the day the choice became wrong.
//!
//! What can be proved is proved instead, and the tests below do exactly three
//! things: that every role is a colour the terminal is **allowed** to remap (an
//! `Indexed` role would silently break the contract above), that no two marks
//! collide, and — the load-bearing one — that [`marked`] changes style and never
//! text.
//!
//! ## Why the glyph chooses the colour
//!
//! `docs/UI.md` §4.4 settles that colour is never load-bearing: it doubles a glyph,
//! it never replaces one. That is easy to state and easy to forget, so [`marked`]
//! makes it structural. It takes a line that is **already formatted and justified**
//! and derives the styling from the marks it finds in it, which means a colour
//! cannot contradict its glyph (it was computed from it) and cannot appear without
//! one (there would be nothing to compute from). The same move [`Mine::grid`] makes
//! by fusing its hole mask into `Option<Block>`: an illegal state made
//! unrepresentable rather than merely discouraged.
//!
//! It also sidesteps a real hazard. Every screen builds its rows with
//! [`format!`] and [`justified`](crate::format::justified), which counts columns
//! across the whole string to align a right-hand field. Splitting a row into
//! [`Span`]s *before* justifying would move that arithmetic into six files and give
//! six chances to get the padding wrong.
//!
//! [`ColourMode`]: crate::palette::ColourMode
//! [`Mine::grid`]: skylode_core::mine::Mine
//! [`Stylize`]: ratatui::style::Stylize

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// The interactive hue: the list cursor `▸`, panel titles, the filled half of a
/// gauge, the scrollbar thumb, the active sub-tab.
///
/// One colour for all of them on purpose — the player learns a single hue as
/// "this is what you are pointing at" instead of a vocabulary of five.
pub const ACCENT: Color = Color::Cyan;

/// The de-emphasised hue: borders, footers, table headers, the unfilled half of a
/// gauge, the scrollbar track.
///
/// `DarkGray` is ANSI bright black, not a dimming modifier. `Modifier::DIM` would
/// have been the other candidate and is avoided: terminals disagree about whether
/// to implement it at all, so a border drawn with it is either subtle or absent
/// depending on the emulator, which is not a choice this module should be making
/// per-machine.
pub const MUTED: Color = Color::DarkGray;

/// `✓` — you can buy it, or, on Levels and Stats, it is already yours.
///
/// One colour for both readings even though `overlay::help`'s legend has to spell
/// out that they differ: they agree on the only thing the colour claims, which is
/// that the row is settled in the player's favour.
pub const AFFORDABLE: Color = Color::Green;

/// `✗` — not enough ore, or a world still behind its level gate.
pub const REFUSED: Color = Color::Red;

/// `~` — the third state: you hold the value but not the denomination, so the
/// purchase wants a compression first.
///
/// Yellow because it is neither of the other two: the design's whole reason for a
/// third glyph is that "cannot afford" was hiding a case the player can fix in two
/// keystrokes, and a colour that leaned green or red would put it back.
pub const COMPRESS_FIRST: Color = Color::Yellow;

/// `●` — where you are now on a roadmap.
///
/// Magenta, and not a second green, because of one row on the Levels screen: the
/// cursor and the current level render adjacent as `▸●`, so this has to separate
/// from [`ACCENT`] beside it *and* from [`AFFORDABLE`] on the reached levels just
/// above it.
pub const CURRENT: Color = Color::Magenta;

/// A panel or modal title: [`ACCENT`], **plus bold**.
///
/// The redundancy rule of `docs/UI.md` §4.4 is about the mine cell, but it applies
/// to chrome for the same reason — bold is what keeps a title reading as a title on
/// a terminal that swallowed the hue.
///
/// A `const` and not a function, which needs ratatui's builders to be `const fn`;
/// they are, so the style is materialised at compile time like the colours beside it.
pub const TITLE: Style = Style::new().fg(ACCENT).add_modifier(Modifier::BOLD);

/// Every role, for the test that pins them all as remappable.
///
/// A test-only constant, in the module rather than in `mod tests`, because it is
/// the *inventory* the doc above claims to be complete — a seventh role added
/// without a line here is exactly what the test cannot catch, and keeping the list
/// next to the constants is what makes the omission visible while writing it.
#[cfg(test)]
const ROLES: [Color; 6] = [ACCENT, MUTED, AFFORDABLE, REFUSED, COMPRESS_FIRST, CURRENT];

/// The closed set of marks, and the role each one takes.
///
/// Four of the five are non-ASCII and appear nowhere else in the interface, so
/// scanning for them cannot produce a false positive. `~` is the exception: it is
/// ASCII and could plausibly turn up in prose one day. Today its only occurrence in
/// the whole crate is `overlay::help`'s legend, where it *is* the mark being
/// explained and colouring it is right — and `the_tilde_is_only_ever_the_mark`
/// pins that, so the phase-5 screens that introduce the real `~` rows find the
/// question already asked rather than discovering it as a rendering bug.
///
/// **`—` is drawn in the mark column and is deliberately not here.** Upgrades › Mines
/// uses it on the rows that have no price to quote — a maxed track, or the End's
/// level gate (`docs/UI.md` §5.4.2) — and it is the one glyph in that column this
/// module does not own. Adding it would be the `~` hazard, except already realised:
/// the em dash is ordinary prose in half the interface, and every one of those lines
/// already passes through [`marked`] — the Stats history (`20:13  Explosive — 9
/// blocks cleared`), the Upgrades detail pane (`Obsidian Mine — richness`), and
/// `overlay::help`'s own legend. One entry here would tint all of them.
///
/// So it renders in the terminal's default foreground, and that is the right answer
/// rather than a gap: `✓ ~ ✗` say what the ore can buy, and `—` says the question was
/// never asked on this row. An absence of an affordability answer has no hue.
const MARKS: [(char, Color); 5] = [
    ('✓', AFFORDABLE),
    ('✗', REFUSED),
    ('~', COMPRESS_FIRST),
    ('●', CURRENT),
    ('▸', ACCENT),
];

/// The style `character` takes as a mark, or `None` if it is ordinary text.
///
/// A linear scan of five entries, which is faster than any lookup structure at this
/// size and, more usefully, keeps [`MARKS`] the single readable statement of what
/// the marks are.
fn mark_style(character: char) -> Option<Style> {
    MARKS
        .iter()
        .find(|(glyph, _)| *glyph == character)
        .map(|(_, colour)| Style::default().fg(*colour))
}

/// `text` as a [`Line`], with every mark in it styled and everything else left alone.
///
/// The input is a **finished** row — already formatted, already justified — and the
/// output holds exactly the same characters in the same order. That is the contract
/// `marking_a_line_changes_its_style_and_never_its_text` exists to hold: the frames
/// in `docs/UI.md` are verified at 80 columns, and a styling pass that could alter a
/// single character would put every one of those counts back in question.
///
/// Returns `Line<'static>` rather than borrowing `text`: the spans own their
/// characters, so callers keep handing over the temporary `format!` string they
/// already build instead of having to keep it alive for the frame.
///
/// A row with no mark in it comes back as one unstyled span — not as a span per
/// character — so the common case costs one allocation and the buffer sees the same
/// default style it would have seen without this call.
pub fn marked(text: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut plain = String::new();

    for character in text.chars() {
        match mark_style(character) {
            Some(style) => {
                // Flush what came before, so the marks stay single-character spans
                // and the text between them is not split for no reason.
                if !plain.is_empty() {
                    spans.push(Span::raw(std::mem::take(&mut plain)));
                }
                spans.push(Span::styled(character.to_string(), style));
            }
            None => plain.push(character),
        }
    }
    if !plain.is_empty() {
        spans.push(Span::raw(plain));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The characters of a line, reassembled from its spans.
    fn text_of(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// The style the span containing `character` was given.
    fn style_of(line: &Line<'_>, character: char) -> Option<Style> {
        line.spans
            .iter()
            .find(|span| span.content.contains(character))
            .map(|span| span.style)
    }

    #[test]
    fn every_role_is_a_named_colour_the_terminal_can_remap() {
        // The whole argument for this module's colour space: a role the player's
        // theme cannot touch is a role that can turn out illegible on their
        // background, and unlike the material palette there is no gate here that
        // would have caught it.
        for role in ROLES {
            assert!(
                !matches!(role, Color::Indexed(_) | Color::Rgb(..)),
                "{role:?} is pinned to an exact colour, so the theme cannot remap it"
            );
        }
    }

    #[test]
    fn no_two_marks_share_a_colour() {
        // Deriving the colour from the glyph is only honest if the map is
        // injective: two marks in one colour would be a colour that says less than
        // the glyph it doubles.
        for (index, (glyph, colour)) in MARKS.iter().enumerate() {
            for (other_glyph, other_colour) in &MARKS[index + 1..] {
                assert_ne!(
                    colour, other_colour,
                    "{glyph} and {other_glyph} are both {colour:?}"
                );
            }
        }
    }

    #[test]
    fn marking_a_line_changes_its_style_and_never_its_text() {
        // Real rows, in the shape the screens actually produce them: the Mines
        // world header, the Levels double mark, an Inventory cursor row, and a
        // Stats gate. If any of these came back a character short or long, every
        // 80-column count in `docs/UI.md` would be back in question.
        for row in [
            " Overworld                        Lv 1  ✓",
            " ▸● 23   The Nether opens                ",
            "  ✓ 15   Iron Mine                       ",
            " ▸ Iron Mine                        12x7 ",
            " Nether     Lv 15                      ✗ ",
            " Cost  6 540 Amethyst    Held  0        ~",
        ] {
            assert_eq!(text_of(&marked(row)), row, "the text moved");
        }
    }

    #[test]
    fn a_line_with_no_mark_is_left_unstyled() {
        // The common case: most rows carry no mark, and they must not pay for this
        // pass — one span, and the default style the buffer would have had anyway.
        let line = marked(" Blocks    76 / 84");
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].style, Style::default());
    }

    #[test]
    fn an_empty_line_produces_no_spans() {
        // A blank spacer row is passed through by several screens; it must not
        // become a span holding nothing.
        assert!(marked("").spans.is_empty());
    }

    #[test]
    fn each_mark_takes_its_own_role() {
        for (glyph, colour) in MARKS {
            let line = marked(&format!(" a {glyph} b"));
            assert_eq!(
                style_of(&line, glyph),
                Some(Style::default().fg(colour)),
                "{glyph} did not take its role"
            );
        }
    }

    #[test]
    fn two_marks_on_one_row_are_styled_apart() {
        // The Levels screen's `▸●`, which is the reason `CURRENT` is not a second
        // green: the two marks are adjacent, so they must not merge into one span.
        let line = marked(" ▸● 23   The Nether opens");
        assert_eq!(style_of(&line, '▸'), Some(Style::default().fg(ACCENT)));
        assert_eq!(style_of(&line, '●'), Some(Style::default().fg(CURRENT)));
    }

    #[test]
    fn the_tilde_is_only_ever_the_mark() {
        // `~` is the one ASCII mark, so it is the one that could collide with
        // prose. This asserts the shape of the rule rather than scanning the crate:
        // a `~` reaching `marked` is a mark, and any screen that needs a literal
        // tilde in prose must keep that line away from this function.
        let line = marked(" ~  compress first");
        assert_eq!(
            style_of(&line, '~'),
            Some(Style::default().fg(COMPRESS_FIRST))
        );
    }

    #[test]
    fn the_em_dash_stays_prose_on_both_of_the_lines_it_appears_on() {
        // The mirror of the test above, and the reason `—` is not in `MARKS`. It is
        // the mark column's "no price here" glyph on one row and ordinary prose on
        // the next, so the two are asserted together: styling it would tint the
        // history line, and the history line is why it cannot be styled.
        for row in [
            "   Stone          Size     maxed        —",
            "20:13  Explosive — 9 blocks cleared",
        ] {
            let line = marked(row);
            // `Some(default)` and not `None`: an unmarked dash is still *in* a span,
            // it is simply in the run of plain text rather than in one of its own.
            assert_eq!(
                style_of(&line, '—'),
                Some(Style::default()),
                "the em dash took a colour: {row:?}"
            );
            assert_eq!(text_of(&line), row, "the text moved");
        }
    }

    #[test]
    fn the_title_style_is_bold_as_well_as_coloured() {
        // The redundancy rule applied to chrome: a terminal that drops the hue must
        // still show a title as a title.
        assert_eq!(TITLE.fg, Some(ACCENT));
        assert!(TITLE.add_modifier.contains(Modifier::BOLD));
    }
}
