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
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
};

use super::centered_rect;
use crate::{
    format::age,
    session::{Splash, SplashRow},
};

/// The block-letter wordmark; the value cell's own art, five rows tall.
const LOGO: [&str; 5] = [
    "███████ ██   ██ ██    ██",
    "██      ██  ██   ██  ██",
    "███████ █████     ████",
    "     ██ ██  ██     ██",
    "███████ ██   ██    ██     L O D E",
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
pub fn render(frame: &mut Frame, area: Rect, splash: &Splash) {
    let [_, logo_area, menu_band, summary_area, fill, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(5),
        // A fixed band whatever the menu holds, so losing `Continue` on a fresh
        // install shortens the box and moves nothing under it.
        Constraint::Length(6),
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    // Left-aligned inside a centred rect, so the block art's columns stay lined up
    // — a per-line centre would jag them.
    let logo_width = LOGO.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
    frame.render_widget(
        Paragraph::new(LOGO.join("\n")),
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

    // The version sits bottom-right, above the key hints on the last row.
    frame.render_widget(
        Paragraph::new(VERSION)
            .alignment(Alignment::Right)
            .block(Block::default().padding(Padding::right(2))),
        fill,
    );
    frame.render_widget(
        Paragraph::new(" ↑↓  select     Enter  confirm     q  quit"),
        footer_area,
    );
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
        // The block art renders as full blocks; at least one row must survive.
        assert!(frame.contains('█'), "the wordmark did not draw: {frame}");
        assert!(frame.contains("L O D E"), "{frame}");
        assert!(frame.contains(VERSION), "{frame}");
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
}
