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
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
};

use super::centered_rect;
use crate::{
    app::{MAX_HEIGHT, MAX_WIDTH},
    format::age,
    session::{CONFIRM_ROWS, ConfirmRow, Splash, SplashRow},
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
const LOGO: [&str; 5] = [
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

/// What each row of the menu says.
fn label(row: SplashRow) -> &'static str {
    match row {
        SplashRow::Continue => "Continue",
        SplashRow::NewGame => "New game",
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
        // install shortens the box and moves nothing under it. Five, which is
        // exactly the three-row box: at six the box sat with one blank row above
        // it and two below, and the gaps either side of it are what the eye reads
        // as the block being centred.
        Constraint::Length(5),
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
    let menu_box = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain);
    let menu_height = u16::try_from(rows.len()).unwrap_or(0).saturating_add(2);
    frame.render_widget(
        Paragraph::new(menu.join("\n")).block(menu_box),
        centered_rect(menu_band, 30, menu_height),
    );

    frame.render_widget(
        Paragraph::new(summary(splash)).alignment(Alignment::Center),
        summary_area,
    );

    // The version sits bottom-right, above the key hints on the last row. It has a
    // row of its own rather than riding the slack: rendered into the fill, it
    // followed the block up and down and landed a third of the way down a tall
    // window instead of in the corner the wireframe puts it in.
    frame.render_widget(
        Paragraph::new(VERSION)
            .alignment(Alignment::Right)
            .block(Block::default().padding(Padding::right(2))),
        version_area,
    );
    frame.render_widget(Paragraph::new(footer(splash)), footer_area);

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
/// asks the player to remember what is at stake; this one prints the level and the
/// pickaxe, which are the two figures the menu was already showing them.
fn confirmation(frame: &mut Frame, area: Rect, splash: &Splash, cursor: usize) {
    let at_stake = splash.resume().map_or_else(String::new, |resume| {
        format!(" Lv {}  ·  {}", resume.level(), resume.pickaxe())
    });
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
fn summary(splash: &Splash) -> String {
    if let Some(resume) = splash.resume() {
        let played = resume.idle().map_or_else(String::new, |idle| {
            format!("\nlast played {} ago", age(idle.as_secs()))
        });
        return format!("Lv {}  ·  {}{played}", resume.level(), resume.pickaxe());
    }
    if !splash.persists() {
        return "Skylode could not find a place to save.\nNothing from this session will be kept."
            .to_owned();
    }
    String::new()
}

#[cfg(test)]
mod tests {
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
    fn a_title_over_a_save_shows_its_level_its_pickaxe_and_how_long_it_sat() {
        let splash = Splash::sample(true, true);
        let frame = crate::overlay::render_to_string(|frame, area| render(frame, area, &splash));
        assert!(frame.contains("Continue"), "{frame}");
        assert!(frame.contains("Lv 23"), "{frame}");
        assert!(frame.contains("Diamond Pickaxe"), "{frame}");
        assert!(frame.contains("last played 3h ago"), "{frame}");
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
        // Not a bare "are you sure?": the two figures the menu was already showing.
        assert!(frame.contains("Lv 23"), "{frame}");
        assert!(frame.contains("Diamond Pickaxe"), "{frame}");
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
