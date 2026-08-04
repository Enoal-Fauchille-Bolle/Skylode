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
//! One consequence falls out for free: the chrome needs **no 16-colour fallback**,
//! because it never asked for more than sixteen — so [`ColourMode`] stays a question
//! about the grid alone.
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
//! one (there would be nothing to compute from). The same move [`Mine`] makes by
//! holding its grid as `Option<Block>` instead of a block table beside a hole mask:
//! an illegal state made unrepresentable rather than merely discouraged.
//!
//! It also sidesteps a real hazard. Every screen builds its rows with
//! [`format!`] and [`justified`](crate::format::justified), which counts columns
//! across the whole string to align a right-hand field. Splitting a row into
//! [`Span`]s *before* justifying would move that arithmetic into six files and give
//! six chances to get the padding wrong.
//!
//! [`ColourMode`]: crate::palette::ColourMode
//! [`Mine`]: skylode_core::mine::Mine

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

/// The colour `glyph` takes as a mark, or [`None`] if it is not one.
///
/// The public face of the table [`marked`] scans, for a caller holding a mark
/// *before* it becomes a character in a row — the Upgrades price block, which tints
/// itself with the hue of the `✓ ~ ✗` that will be justified onto its last line.
/// Reading one table is what stops a block and its own mark disagreeing.
///
/// **Takes the glyph and not a `Mark`, which keeps this module a leaf.**
/// [`Mark`](crate::view::Mark) lives in the read model, and a colour table that
/// imported it would put the projection and the palette in a cycle. It also means
/// `Mark::Owned`'s empty string and `Mark::NoPrice`'s `—` answer `None` without a
/// second rule: the em dash is deliberately absent from [`MARKS`], so a row that has
/// no affordability answer gets no hue for the same reason its glyph gets none.
///
/// A multi-character string is never a mark. Every entry in [`MARKS`] is one column
/// wide, so a longer glyph is prose and must not be tinted by its first letter.
pub fn of_glyph(glyph: &str) -> Option<Color> {
    let mut characters = glyph.chars();
    let character = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    MARKS
        .iter()
        .find(|(mark, _)| *mark == character)
        .map(|&(_, colour)| colour)
}

/// `text` as a [`Line`], with every mark in it styled and everything else left alone.
///
/// The plain case of [`marked_row`], and the one every screen but Upgrades' detail
/// pane wants.
pub fn marked(text: &str) -> Line<'static> {
    marked_row(text, 0, None)
}

/// `text` as a [`Line`]: the first `label_columns` columns muted, the rest in `tint`,
/// and every mark still taking its own hue.
///
/// The input is a **finished** row — already formatted, already justified — and the
/// output holds exactly the same characters in the same order. That is the contract
/// `marking_a_line_changes_its_style_and_never_its_text` exists to hold: the frames
/// in `docs/UI.md` are verified at 80 columns, and a styling pass that could alter a
/// single character would put every one of those counts back in question. It is also
/// why this takes two *numbers* beside the string rather than taking spans: the
/// padding is a property of the whole row, so splitting it before measuring would move
/// [`justified`](crate::format::justified)'s arithmetic into six files.
///
/// **The mark scan wins over both.** A tinted row's `✗` is still `REFUSED` and its `▸`
/// still `ACCENT`, because `mark_style` is consulted first and the base is only the
/// fallback. That is what keeps §4.5's *"the colour of a mark is derived from the
/// mark"* true of a tinted row as well as a plain one — a tint can never quietly
/// recolour an answer.
///
/// `label_columns` is counted in **columns and not bytes**, like everything else that
/// measures a row here, so a label following a multi-byte mark still ends where the
/// caller counted it.
///
/// Returns `Line<'static>` rather than borrowing `text`: the spans own their
/// characters, so callers keep handing over the temporary `format!` string they
/// already build instead of having to keep it alive for the frame.
///
/// Runs of one style are merged, so a plain row still comes back as a single unstyled
/// span and the buffer sees the style it would have seen without this call.
pub fn marked_row(text: &str, label_columns: usize, tint: Option<Color>) -> Line<'static> {
    styled(text, |column| {
        if column < label_columns {
            Style::default().fg(MUTED)
        } else {
            tint.map_or_else(Style::default, |colour| Style::default().fg(colour))
        }
    })
}

/// `text` as a [`Line`] with everything from column `from` onward muted.
///
/// **[`marked_row`]'s rule with the halves the other way round**, for a row whose
/// secondary column comes *last*. The recovery screen annotates each choice with its
/// consequence at a fixed column — `Restore the backup    saved 1m ago` — and there it
/// is the annotation that steps back and the choice that keeps the foreground, which is
/// the opposite arrangement to the Upgrades pane's `Cost   40 Redstone`.
///
/// One rule survives the swap unchanged, and it is the load-bearing one: the **mark
/// scan still wins**, so a `▸` or a `✗` falling in the muted half keeps its own hue.
/// §4.5's *"the colour of a mark is derived from the mark"* cannot be switched off by
/// where on the row the mark happens to land.
pub fn marked_tail(text: &str, from: usize) -> Line<'static> {
    styled(text, |column| {
        if column < from {
            Style::default()
        } else {
            Style::default().fg(MUTED)
        }
    })
}

/// The shared body of [`marked_row`] and [`marked_tail`]: walk the columns, ask `base`
/// what each one would be, and let a mark override it.
///
/// **Extracted rather than duplicated**, because the run-merging below is the part that
/// must not drift: two copies would be two chances to split a `▸●` into one span or to
/// leave a plain row carrying spans it does not need. `base` is a closure over the
/// column index, which is what lets the two callers disagree about *where* the muting
/// goes without disagreeing about anything else.
fn styled(text: &str, base: impl Fn(usize) -> Style) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run: Option<(Style, String)> = None;

    for (column, character) in text.chars().enumerate() {
        let style = mark_style(character).unwrap_or_else(|| base(column));
        match &mut run {
            Some((current, buffered)) if *current == style => buffered.push(character),
            _ => {
                if let Some((current, buffered)) = run.take() {
                    spans.push(Span::styled(buffered, current));
                }
                run = Some((style, character.to_string()));
            }
        }
    }
    if let Some((current, buffered)) = run {
        spans.push(Span::styled(buffered, current));
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
    fn a_glyph_answers_with_the_same_hue_the_scan_would_give_it() {
        // The one table, read from both ends: `of_glyph` is what the Upgrades price
        // block tints itself with, and `marked_row` is what colours the mark beside it.
        // Two tables here would let a red price sit under a green tick.
        for (glyph, colour) in [('✓', AFFORDABLE), ('~', COMPRESS_FIRST), ('✗', REFUSED)] {
            assert_eq!(of_glyph(&glyph.to_string()), Some(colour));
            let row = marked(&format!(" Cost      40 Redstone   {glyph}"));
            assert_eq!(style_of(&row, glyph), Some(Style::default().fg(colour)));
        }
    }

    #[test]
    fn the_glyphs_with_no_verdict_to_state_have_no_hue() {
        // `Mark::Owned` is the empty string and `Mark::NoPrice` is the em dash, which
        // this module deliberately does not own. Both must answer `None` rather than
        // falling through to some default, or a settled row and an unpriced one would
        // be tinted like a refusal.
        assert_eq!(of_glyph(""), None);
        assert_eq!(of_glyph("—"), None);
        // And a word is never a mark, however it starts.
        assert_eq!(of_glyph("✓ or not"), None);
    }

    #[test]
    fn a_tint_colours_the_text_and_never_the_mark_on_it() {
        // The rule §4.5 makes structural, extended to a tinted row: the scan runs last,
        // so a price drawn in `REFUSED` still carries a `✓` in `AFFORDABLE` if that is
        // what the glyph says. A tint that could override a mark would be a colour
        // contradicting its own glyph — the one thing this module exists to forbid.
        let line = marked_row(" Cost      40 Redstone   ✓", 0, Some(REFUSED));

        assert_eq!(style_of(&line, '4'), Some(Style::default().fg(REFUSED)));
        assert_eq!(style_of(&line, '✓'), Some(Style::default().fg(AFFORDABLE)));
    }

    #[test]
    fn a_label_is_muted_and_the_value_after_it_is_not() {
        // `block` writes eleven columns of margin, label and gap; everything past them
        // is the answer the player came for. The boundary is a count of *columns*, like
        // every other measurement on a row.
        let line = marked_row(" Cost      1 Compressed Stone", 11, None);

        assert_eq!(style_of(&line, 'C'), Some(Style::default().fg(MUTED)));
        assert_eq!(style_of(&line, '1'), Some(Style::default()));
        assert_eq!(text_of(&line), " Cost      1 Compressed Stone");
    }

    #[test]
    fn a_row_with_neither_a_label_nor_a_tint_is_what_marked_already_drew() {
        // The compatibility claim that lets `marked` be one call into `marked_row`:
        // every screen but the Upgrades pane passes zero and `None`, and none of them
        // may change appearance because of it.
        let row = " ▸ Netherite Eff XV                ✗";
        assert_eq!(marked(row), marked_row(row, 0, None));
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
