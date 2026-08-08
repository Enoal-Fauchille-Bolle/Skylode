//! The splash / main menu (UI.md §6.1).
//!
//! The first screen, and per the bootstrap rule (§2.3) its **chrome is hardcoded**
//! while its **summary is save-derived**: `Continue` and the two summary lines are
//! absent on a fresh install. What arrives here is a [`Splash`] — a menu, a caret and
//! at most one loaded save's headline — and never a [`Config`](crate::config::Config),
//! because config lives inside a save that may be missing or refused, and reading
//! preferences out of a file the game has not trusted yet is a contradiction.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Block, Padding, Paragraph},
};

use super::centered_rect;
use crate::{
    app::{MAX_HEIGHT, MAX_WIDTH},
    format::{age, prestige_rank},
    session::{CONFIRM_ROWS, ConfirmRow, Resume, Splash, SplashRow},
    theme,
};

/// The block-letter wordmark: the whole word, five rows of a 55-column design.
///
/// **Every row belongs to the same box**, and that is a layout requirement rather
/// than a taste: the art is drawn left-aligned inside a rect [`centered_rect`] sizes
/// from the *widest* row, so that width is what defines the wordmark's centre. The
/// art this replaced hung ` L O D E` off the end of its last row alone, making that
/// row twelve columns wider than the block above it — so the block was centred as if
/// it were twelve columns wider than it looked, and drew left of centre with a tail
/// off to the right. One word, one bounding box, and the bug cannot come back.
///
/// 55 columns is deliberate too: it clears the 80-column budget
/// [`too_small`](super::too_small) enforces with twelve columns of margin either
/// side, so the title needs no narrow variant.
pub(crate) const LOGO: [&str; 5] = [
    "███████ ██   ██ ██   ██ ██      ███████ ██████  ███████",
    "██      ██  ██   ██ ██  ██      ██   ██ ██   ██ ██",
    "███████ █████     ███   ██      ██   ██ ██   ██ ██████",
    "     ██ ██  ██     ██   ██      ██   ██ ██   ██ ██",
    "███████ ██   ██    ██   ███████ ███████ ██████  ███████",
];

/// The version, taken from the manifest rather than written out.
///
/// `env!` reads it at compile time, so the corner of the title screen cannot drift
/// from what cargo thinks this build is — which the hardcoded `skylode 0.1.0` it
/// replaces could, silently, on the first release.
const VERSION: &str = concat!("skylode ", env!("CARGO_PKG_VERSION"));

/// The one line that names a run: `Lv 23 · Diamond Eff V · Prestige II`.
///
/// **Written once because two places print it**, and they are required to agree: the
/// summary under the menu says what there is to continue, and the confirmation box says
/// what a `New game` would cost. Two `format!`s would let the box quote back a run
/// described in different terms from the one the player was just reading.
///
/// **The prestige segment is absent at rank 0, and that is the common case** — every
/// run before its first prestige, which the balance pass puts at one to a bit over two
/// hours. `Prestige 0` would spend the headline's third slot announcing something the
/// player has *not* done; the two figures that survive are the two that are true of
/// every run. Note that this is the opposite call from the Stats panel, which prints
/// `rank 0` without hesitating: that panel is a readout of every figure, and this line
/// is a headline.
///
/// The rank goes through [`prestige_rank`], not [`roman`](crate::format::roman), so a
/// player past the numerals reads `Prestige 16` here rather than a `?` — see that
/// function for why a prestige rank has no cap to spell out.
fn headline(resume: &Resume) -> String {
    let mut line = format!("Lv {} · {}", resume.level(), resume.pickaxe());
    if resume.prestige() > 0 {
        line.push_str(&format!(" · Prestige {}", prestige_rank(resume.prestige())));
    }
    line
}

/// What each row of the menu says.
fn label(row: SplashRow) -> &'static str {
    match row {
        SplashRow::Continue => "Continue",
        SplashRow::NewGame => "New game",
        SplashRow::Settings => "Settings",
        SplashRow::Quit => "Quit",
    }
}

/// Draws the splash screen.
///
/// **The title obeys the same width cap as the screens behind it.** It is drawn
/// straight from [`Session`](crate::session::Session) rather than through
/// [`App::render`](crate::app::App::render), so it does not inherit that band for
/// free — and without it, a 240-column terminal put the version corner and the key
/// hints at opposite ends of the desk. Below the caps this is the identity, so the
/// counted 80×24 frame is untouched.
pub fn render(frame: &mut Frame, area: Rect, splash: &Splash) {
    let area = area.centered(Constraint::Max(MAX_WIDTH), Constraint::Max(MAX_HEIGHT));

    // Where the slack goes is the whole of vertical centring in ratatui: `Length`
    // asks for its rows, `Fill` absorbs what is left, and two `Fill`s share it in
    // proportion. 2 above and 3 below is the **optical** centre — the eye reads the
    // middle of a frame as slightly above its arithmetic middle, so a title placed
    // dead centre looks like it has slipped. The version and the footer sit outside
    // both fills, pinned to the last two rows whatever the window does.
    let [
        _,
        logo_area,
        _,
        menu_band,
        _,
        summary_area,
        _,
        version_area,
        footer_area,
    ] = Layout::vertical([
        Constraint::Fill(2),
        Constraint::Length(5),
        Constraint::Length(1),
        // A fixed band whatever the menu holds, so losing `Continue` on a fresh
        // install shortens the box and moves nothing under it. It is sized to the
        // **longest** menu — four rows plus two border lines — because
        // `centered_rect` clamps to the band it is given, so a band shorter than the
        // box would not centre it but cut its last row off. It grew from five with
        // the `Settings` row; before that the longest menu was three rows.
        Constraint::Length(6),
        Constraint::Length(1),
        Constraint::Length(2),
        Constraint::Fill(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Left-aligned inside a centred rect, so the block art's columns stay lined up
    // — a per-line centre would jag them. See [`LOGO`] for why every row has to
    // share one bounding box for this to land where it looks like it should.
    let logo_width = LOGO.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
    frame.render_widget(
        Paragraph::new(LOGO.join("\n")).style(Style::default().fg(theme::ACCENT)),
        centered_rect(logo_area, logo_width, 5),
    );

    let rows = splash.rows();
    let menu: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let caret = if index == splash.cursor() {
                "  ▸  "
            } else {
                "     "
            };
            format!("{caret}{}", label(*row))
        })
        .collect();
    // `square("")` and not a hand-rolled `Block`: the same square, muted border every
    // other box in the boot-and-modal class takes. The border it replaces was drawn in
    // the terminal's default foreground, which made the title the one screen where a
    // box competed with its own contents.
    let menu_height = u16::try_from(rows.len()).unwrap_or(0).saturating_add(2);
    frame.render_widget(
        // Through `theme::marked`, like every list row in the crate: the caret is a
        // mark, so it takes ACCENT without this module naming a colour. The `▸` in the
        // confirmation box below already did — it goes through `modal` — so the title
        // used to draw two carets in two different colours on the same screen.
        Paragraph::new(
            menu.iter()
                .map(|row| theme::marked(row))
                .collect::<Vec<_>>(),
        )
        .block(super::square("")),
        centered_rect(menu_band, 30, menu_height),
    );

    summary(frame, summary_area, splash);

    // The version sits bottom-right, above the key hints on the last row. It has a
    // row of its own rather than riding the slack: rendered into the fill, it
    // followed the block up and down and landed a third of the way down a tall
    // window instead of in the corner the wireframe puts it in.
    frame.render_widget(
        Paragraph::new(VERSION)
            .alignment(Alignment::Right)
            .style(Style::default().fg(theme::MUTED))
            .block(Block::default().padding(Padding::right(2))),
        version_area,
    );
    // Muted, like every other footer in the crate — see `screen::mine::footer`, which
    // spells out the argument: a key hint is the least urgent thing on screen, and the
    // de-emphasised hue is what lets the rows above it be read without competition.
    frame.render_widget(
        Paragraph::new(footer(splash)).style(Style::default().fg(theme::MUTED)),
        footer_area,
    );

    // Last, so it covers the menu it is asking about. A modal clears its own rect, so
    // the title underneath costs nothing and is still readable around it.
    if let Some(cursor) = splash.confirm() {
        confirmation(frame, area, splash, cursor);
    }
}

/// The key hints on the bottom row.
///
/// They change with the box, because the box changes what the keys do: `Esc` exists
/// only while there is something to decline, and advertising it on a title that cannot
/// use it would be a fourth hint for a key that does nothing.
fn footer(splash: &Splash) -> &'static str {
    if splash.confirm().is_some() {
        return " ↑↓  select     Enter  confirm     Esc  keep the save";
    }
    " ↑↓  select     Enter  confirm     q  quit"
}

/// The *"start a new game?"* box (§6.1, and a departure from it — see [`Splash`]).
///
/// **It names the run it would cost.** A confirmation that only says *"are you sure?"*
/// asks the player to remember what is at stake; this one prints [`headline`], the
/// very line the menu was already showing them — the same function and not a second
/// wording, so the box cannot describe the run in terms the summary did not use.
fn confirmation(frame: &mut Frame, area: Rect, splash: &Splash, cursor: usize) {
    let at_stake = splash
        .resume()
        .map_or_else(String::new, |resume| format!(" {}", headline(resume)));
    let rows: Vec<String> = CONFIRM_ROWS
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let caret = if index == cursor { " ▸  " } else { "    " };
            let label = match row {
                ConfirmRow::Keep => "No, keep this save",
                ConfirmRow::StartOver => "Yes, start over",
            };
            format!("{caret}{label}")
        })
        .collect();

    let mut lines = vec![
        String::new(),
        " There is already a run here:".to_owned(),
        at_stake,
        String::new(),
        // What actually happens, and when. The old run is not deleted on the spot —
        // it goes when the new one first writes, ten seconds later — but a sentence
        // that offered that as a window would be inviting the player to race it.
        " Starting over writes over it.".to_owned(),
        String::new(),
    ];
    lines.extend(rows);
    lines.push(String::new());

    let height = u16::try_from(lines.len())
        .unwrap_or(u16::MAX)
        .saturating_add(2);
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    super::modal(frame, area, 52, height, " Start a new game? ", &borrowed);
}

/// The two lines under the menu: what there is to continue, or why there is not.
///
/// **Three cases, and one of them is silence.** A save gives its headline; a session
/// that cannot write at all says so *here*, because this is where the missing
/// `Continue` needs explaining and a toast three screens later would not; and a fresh
/// install on a working disk says nothing, since "no save yet" is already what a menu
/// without `Continue` means.
///
/// The age is [`age`]'s `1h`, not the wireframe's `1 hour` — one vocabulary for every
/// elapsed time in the crate, the same one the Stats history prints.
///
/// **The headline is drawn plainly and the age is muted**, which is the hierarchy
/// [`theme::marked_row`] already applies to every row with a label and a value: the
/// figures the player came for keep the foreground, the context around them steps
/// back. Here the figures are [`headline`]'s — the same line the confirmation box
/// quotes back when a `New game` would cost them — and *when* the run was last
/// touched is what surrounds them.
///
/// **The "nowhere to save" warning takes no colour at all**, and that is `docs/UI.md`
/// §4.4 rather than an omission: colour doubles a glyph and never replaces one, and
/// this sentence has no glyph to double. Drawing it in [`theme::REFUSED`] would make
/// it the one place in the interface where a hue carries a meaning on its own — and
/// the sentence already says the whole thing without help.
fn summary(frame: &mut Frame, area: Rect, splash: &Splash) {
    let muted = Style::default().fg(theme::MUTED);
    let lines: Vec<Line<'static>> = if let Some(resume) = splash.resume() {
        let mut lines = vec![Line::from(headline(resume))];
        if let Some(idle) = resume.idle() {
            lines.push(Line::styled(
                format!("last played {} ago", age(idle.as_secs())),
                muted,
            ));
        }
        lines
    } else if splash.persists() {
        Vec::new()
    } else {
        vec![
            Line::from("Skylode could not find a place to save."),
            Line::from("Nothing from this session will be kept."),
        ]
    };

    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

#[cfg(test)]
mod tests {
    use ratatui::style::Color;

    use super::*;

    #[test]
    fn a_fresh_install_offers_no_continue_and_says_nothing_about_a_save() {
        let splash = Splash::sample(true, false);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        assert!(
            frame.contains("New game") && frame.contains("Quit"),
            "{frame}"
        );
        assert!(
            !frame.contains("Continue"),
            "a fresh install was offered a run: {frame}"
        );
        assert!(
            !frame.contains("could not find a place"),
            "a working disk complained: {frame}"
        );
    }

    #[test]
    fn it_draws_the_wordmark_and_the_build_it_is() {
        let splash = Splash::sample(true, false);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        // Every row of the art, not just "some block glyph reached the screen": a
        // wordmark clipped on the right would still pass the loose version.
        for row in LOGO {
            assert!(frame.contains(row), "the wordmark row {row:?} did not draw");
        }
        assert!(frame.contains(VERSION), "{frame}");
    }

    /// The bug the one-bounding-box rule exists for.
    ///
    /// The art is left-aligned inside a rect sized from its widest row, so a row that
    /// is wider than the block *looks* pushes the block off-centre by half the
    /// difference. Asserting on the margins rather than on a column number is what
    /// keeps this true if the art is ever redrawn at another width.
    #[test]
    fn the_wordmark_sits_centred_rather_than_hung_off_its_widest_row() {
        let splash = Splash::sample(true, true);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        let row = frame
            .lines()
            .find(|line| line.contains(LOGO[0]))
            .unwrap_or_default();
        let left = row.len() - row.trim_start().len();
        let right = row.len() - row.trim_end().len();
        assert!(
            left.abs_diff(right) <= 1,
            "the wordmark is off-centre by {} columns: {row:?}",
            left.abs_diff(right)
        );
    }

    /// On a window with room to spare the block floats; the corners do not.
    ///
    /// 80×48 is twice the counted height, so there are ~31 rows of slack to place —
    /// enough that a top-anchored layout and a centred one are unmistakably different.
    #[test]
    fn a_tall_window_lifts_the_block_off_the_top_and_leaves_the_corners_pinned() {
        let splash = Splash::sample(true, true);
        let frame = crate::overlay::render_to_string_sized(80, 48, |frame, area| {
            render(frame, area, &splash);
        });
        let rows: Vec<&str> = frame.lines().collect();

        let logo_row = rows
            .iter()
            .position(|line| line.contains(LOGO[0]))
            .unwrap_or_default();
        // Well clear of the top — the old layout drew it on row 1 at every height.
        assert!(
            logo_row > 6,
            "the block is still anchored to the top: {frame}"
        );
        // And above the arithmetic middle, which is the optical centre's whole point.
        assert!(
            logo_row < 24,
            "the block sank to or below the middle: {frame}"
        );

        // The version and the hints stay in the last two rows however tall it gets.
        assert!(
            rows.get(46).is_some_and(|line| line.contains(VERSION)),
            "the version did not stay pinned above the footer: {frame}"
        );
        assert!(
            rows.get(47).is_some_and(|line| line.contains("q  quit")),
            "the footer did not stay on the last row: {frame}"
        );
    }

    /// **The title drawn to the same rules as the six screens behind it.**
    ///
    /// Every claim in one draw, because they are one claim: the chrome is a system, and
    /// a test per constant would let three of them pass while the fourth drifted. The
    /// caret is the load-bearing one — before this, the `▸` in the confirmation box
    /// (which goes through `modal`, so through `theme::marked`) was accented while the
    /// `▸` in the menu one row above it was not, on the same screen.
    #[test]
    fn the_title_takes_the_same_chrome_colours_as_every_other_screen() {
        let splash = Splash::sample(true, true);
        let buffer = crate::overlay::render_to_buffer(80, 24, |frame, area| {
            render(frame, area, &splash);
        });
        let colour_of = |needle| crate::overlay::colour_of(&buffer, needle);

        assert_eq!(colour_of("▸"), Some(theme::ACCENT), "the caret");
        assert_eq!(colour_of("┌"), Some(theme::MUTED), "the menu box border");
        assert_eq!(colour_of("↑"), Some(theme::MUTED), "the footer");
        assert_eq!(colour_of("█"), Some(theme::ACCENT), "the wordmark");

        // The summary's hierarchy: the two figures the player came for keep the
        // foreground, the age behind them steps back. The probes are letters no
        // earlier row carries — `D` from `Diamond Pickaxe`, `y` from `last played`.
        // Not `g`: `New game` sits three rows above and would answer first.
        assert_eq!(colour_of("D"), Some(Color::Reset), "the headline");
        assert_eq!(colour_of("y"), Some(theme::MUTED), "the age");
    }

    /// §4.4 held where it is easiest to break: a warning with no glyph takes no hue.
    #[test]
    fn the_nowhere_to_save_warning_carries_its_meaning_in_words_and_not_in_a_colour() {
        let splash = Splash::sample(false, false);
        let buffer = crate::overlay::render_to_buffer(80, 24, |frame, area| {
            render(frame, area, &splash);
        });
        // `k` appears first in `Skylode could not find…`, which is the warning's own
        // first line — the footer below it has none before that point.
        assert_eq!(
            crate::overlay::colour_of(&buffer, "k"),
            Some(Color::Reset),
            "the warning took a hue, making colour load-bearing for the first time"
        );
    }

    /// The width cap, which the title does not inherit from [`crate::app::App`].
    #[test]
    fn a_very_wide_window_keeps_the_corners_within_the_band_rather_than_at_its_edges() {
        let splash = Splash::sample(true, true);
        let width = MAX_WIDTH + 60;
        let frame = crate::overlay::render_to_string_sized(width, 30, |frame, area| {
            render(frame, area, &splash);
        });
        let margin = usize::from((width - MAX_WIDTH) / 2);
        let footer = frame.lines().last().map(str::to_owned).unwrap_or_default();
        assert_eq!(
            footer.len() - footer.trim_start().len(),
            margin + 1,
            "the hints ran to the terminal's edge instead of the band's: {footer:?}"
        );
    }

    #[test]
    fn a_session_that_cannot_write_says_so_where_the_summary_goes() {
        let splash = Splash::sample(false, false);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        assert!(frame.contains("could not find a place to save"), "{frame}");
        assert!(frame.contains("Nothing from this session"), "{frame}");
    }

    #[test]
    fn a_title_over_a_save_shows_its_level_its_pickaxe_its_rank_and_how_long_it_sat() {
        let splash = Splash::sample(true, true);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        assert!(frame.contains("Continue"), "{frame}");
        // One assertion on the whole line rather than three on its pieces: what the
        // headline promises is an *order* and a separator, and three `contains` would
        // pass just as well on three figures scattered down the screen.
        assert!(
            frame.contains("Lv 23 · Diamond Pickaxe · Prestige II"),
            "{frame}"
        );
        assert!(frame.contains("last played 3h ago"), "{frame}");
    }

    /// The headline's one branch: at rank 0 the segment is not there to read.
    ///
    /// Asserting the *absence* of `Prestige` and not just the presence of the two
    /// figures beside it, because the bug this guards against is a `Prestige 0` shown
    /// to every player who has not prestiged yet — and a frame carrying it would
    /// satisfy any assertion written about the level and the pickaxe.
    #[test]
    fn a_run_before_its_first_prestige_keeps_the_rank_off_the_headline() {
        let splash = Splash::sample_at_rank(true, true, 0);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        assert!(frame.contains("Lv 23 · Diamond Pickaxe"), "{frame}");
        assert!(
            !frame.contains("Prestige"),
            "a run that has never prestiged was told its rank: {frame}"
        );
    }

    /// Past the numerals a rank still reads as a number, never as `roman`'s `?`.
    #[test]
    fn a_rank_past_the_roman_table_prints_as_a_number_on_the_title_too() {
        let splash = Splash::sample_at_rank(true, true, 42);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        assert!(frame.contains("Prestige 42"), "{frame}");
    }

    /// **The longest menu fits in its band.**
    ///
    /// `centered_rect` *clamps* a box to the area it is given rather than overflowing
    /// it, so a band one row short of the box does not misplace the menu — it silently
    /// cuts the last row off. Adding `Settings` took the longest menu from three rows to
    /// four, and this is what says the band grew with it. Asserted on `Quit`, which is
    /// the row that would go, and on the bottom border, which is what would go with it.
    #[test]
    fn the_longest_menu_keeps_its_last_row_and_its_own_bottom_border() {
        let splash = Splash::sample(true, true);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        for row in ["Continue", "New game", "Settings", "Quit"] {
            assert!(frame.contains(row), "{row} was cut off the menu: {frame}");
        }
        // The box is closed: four rows plus two borders, so the `└` has to be there.
        assert!(frame.contains('└'), "the menu box lost its floor: {frame}");
    }

    #[test]
    fn the_caret_marks_the_row_it_is_on_and_only_that_row() {
        let splash = Splash::sample(true, true);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        assert_eq!(
            frame.matches('▸').count(),
            1,
            "more than one row was marked: {frame}"
        );
        // The caret opens on the first row, which is `Continue` when there is one.
        let marked = frame
            .lines()
            .find(|line| line.contains('▸'))
            .unwrap_or_default();
        assert!(marked.contains("Continue"), "{frame}");
    }

    #[test]
    fn the_confirmation_names_the_run_it_would_cost_and_opens_on_keeping_it() {
        let splash = Splash::sample_confirming();
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        assert!(frame.contains("Start a new game?"), "{frame}");
        // Not a bare "are you sure?": the very line the menu was already showing,
        // prestige rank included — the figure a `New game` costs most dearly.
        assert!(
            frame.contains("Lv 23 · Diamond Pickaxe · Prestige II"),
            "{frame}"
        );
        assert!(frame.contains("writes over it"), "{frame}");

        let marked = frame
            .lines()
            .find(|line| line.contains('▸'))
            .unwrap_or_default();
        assert!(
            marked.contains("No, keep this save"),
            "a stray Enter would have destroyed the run: {frame}"
        );
        // And the footer advertises the key that backs out, which exists only here.
        assert!(frame.contains("Esc  keep the save"), "{frame}");
    }
}
