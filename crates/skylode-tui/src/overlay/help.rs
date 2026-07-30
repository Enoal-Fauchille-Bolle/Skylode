//! The Help screen (UI.md §6.11).
//!
//! **Full screen, not a modal box**, because ~20 bindings plus the legend do not
//! fit a centred modal without scrolling, and an aid that scrolls is one whose
//! bottom gets missed. It prints the globals plus **only the screen it was opened
//! from** — the question a player opens Help to ask is almost always about the
//! screen in front of them — and the legend for the glyphs a screen shows.
//!
//! Two things are dynamic. The **sub-tab binding is rendered from config**: an aid
//! that showed the default while the player chose an alternative teaches a dead key.
//! And the contextual block follows `screen`, so opening Help from Upgrades lists
//! the sub-tab keys while opening it from Mine does not.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Clear, Paragraph},
};

use super::square;
use crate::{config::Config, screen::Screen, theme};

/// The globals — the "Anywhere" block, shown whatever screen Help was opened from.
///
/// **`q` reads `back to title screen` and not `back to the title screen`**, which is
/// a row bought with an article. The pane is twenty-one rows and Upgrades filled all
/// of them; the fifth binding that screen now has had to come from somewhere, and the
/// choice was between dropping a `the` and dropping a key from the list. Twenty-three
/// columns is what the label column leaves a value here, and the longer form is
/// twenty-four — so it wrapped, at the cost of a whole row, to say nothing more.
const GLOBALS: [&str; 9] = [
    "",
    "Anywhere",
    " Tab  ⇧Tab     next / previous screen",
    " 1 … 6         jump to a screen",
    " ← →           adjust the value under",
    "               the cursor",
    " s             Settings",
    " ?             this help",
    " q             back to title screen",
];

/// The Mining block — shown always, since mining is the game's core act and must be
/// findable from any screen (UI.md §6.11 draws it even when opened from Upgrades).
const MINING: [&str; 4] = [
    "Mining",
    " Space         hold to mine. Settings",
    "               can make it press to",
    "               start, press to stop.",
];

/// The right pane: the glyph legend, fixed whatever the screen.
///
/// **Twenty-one lines, and that is the pane's whole height**, not a coincidence: at
/// the reference 80×24 the footer takes one row and the box's own borders two, which
/// leaves exactly this many. A `Paragraph` clips what does not fit *in silence* — no
/// panic, no warning, just a missing line — and this legend had been one line over
/// for a while without anyone seeing it, because the line that fell off the bottom
/// was the last one. `the_legend_fits_the_pane_at_the_reference_size` asserts on that
/// last line for exactly that reason: it is the one the overflow eats first.
///
/// The length is spelled out rather than inferred so that adding an entry is a build
/// error until it has been counted — the type is the inventory, the same job
/// `theme::MARKS` does for the colours these glyphs take.
const LEGEND: [&str; 21] = [
    "",
    "The mine grid",
    "  a solid colour  an intact cell, in",
    "                  its material colour",
    "  a stippled cell the cell of value,",
    "                  in every colour mode",
    "  · : #           the cell you are",
    "                  breaking, filling up",
    "  nothing at all  already broken",
    "",
    "Marks",
    "  ✓   you can buy it",
    "  ~   you hold the ore but not the",
    "      denomination — compress first",
    "  ✗   not enough ore",
    "  ●   where you are now",
    "  —   nothing to buy: maxed, or gated",
    "",
    "  On Levels and on Stats, ✓ reads",
    "  \"already yours\": nothing is bought",
    "  on those two screens.",
];

/// Draws the Help screen for the screen it was opened from.
pub fn render(frame: &mut Frame, area: Rect, screen: Screen, config: &Config) {
    let [body, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    // `40 : 40`, the counted split, written unreduced like every other one in the
    // crate — see the rule on `crate::screen`.
    let [left, right] =
        Layout::horizontal([Constraint::Fill(40), Constraint::Fill(40)]).areas(body);

    let mut keys: Vec<String> = GLOBALS.iter().map(|line| (*line).to_owned()).collect();
    let context = contextual(screen, config);
    if !context.is_empty() {
        keys.push(String::new());
        keys.push(format!("On this screen — {}", screen.title()));
        keys.extend(context);
    }
    keys.push(String::new());
    keys.extend(MINING.iter().map(|line| (*line).to_owned()));

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(keys.join("\n")).block(square(" Keys ")),
        left,
    );
    // The legend through `marked`, which makes it demonstrate what it explains: the
    // `✓` on the line reading "you can buy it" is drawn in the very colour the
    // player will meet on the Upgrades list. A legend printing its marks in a
    // different colour from the screens would be teaching the wrong thing.
    let legend: Vec<Line<'static>> = LEGEND.iter().map(|line| theme::marked(line)).collect();
    frame.render_widget(
        Paragraph::new(legend).block(square(" Reading the screen ")),
        right,
    );
    frame.render_widget(
        Paragraph::new(" Esc  or  ?   close").style(Style::default().fg(theme::MUTED)),
        footer_area,
    );
}

/// The bindings unique to `screen` — the "On this screen" block, empty on Mine,
/// whose only key (`Space`) already lives under Mining.
///
/// This is the one place the config binding surfaces: Upgrades' `switch sub-tab`
/// line is drawn from `config`, not from the hardcoded default.
fn contextual(screen: Screen, config: &Config) -> Vec<String> {
    // Owned lines rather than `&'static str`, because the Upgrades arm interpolates
    // the config binding — so the whole set is `String` and the arms stay uniform.
    let owned = |lines: &[&str]| lines.iter().map(|l| (*l).to_owned()).collect();
    match screen {
        Screen::Mine => Vec::new(),
        Screen::Mines => owned(&[
            " ↑ ↓           select a mine",
            " Enter         mine it",
            " ← →           move the richness dial",
        ]),
        Screen::Inventory => owned(&[
            " ↑ ↓           select a material",
            " c             compress",
            " C             decompress",
        ]),
        Screen::Upgrades => vec![
            format!(" {:<14}switch sub-tab", config.sub_tab_keys.label()),
            " ↑ ↓           select a row".to_owned(),
            " Enter         buy up to the cursor".to_owned(),
            " M             buy as many as you can".to_owned(),
            // The §8.4 return leg. Listed under Upgrades and not under Inventory
            // because it is pressed *here* — the same letter, one screen earlier.
            " c             go compress what is short".to_owned(),
        ],
        Screen::Stats => owned(&[
            " ↑ ↓           scroll the history",
            " p             prestige",
        ]),
        Screen::Levels => owned(&[" ↑ ↓           scroll", " Home          jump to your level"]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SubTabKeys;

    /// Renders Help opened from `screen` with `config`, as one frame string.
    fn help(screen: Screen, config: &Config) -> String {
        crate::overlay::render_to_string(|frame, area| render(frame, area, screen, config))
    }

    #[test]
    fn it_shows_the_globals_and_the_legend_whatever_the_screen() {
        let frame = help(Screen::Mine, &Config::default());
        assert!(frame.contains("Anywhere"), "{frame}");
        assert!(frame.contains("next / previous screen"), "{frame}");
        assert!(frame.contains("Reading the screen"), "{frame}");
        assert!(frame.contains("already yours"), "{frame}");
        assert!(frame.contains("Esc  or  ?   close"), "{frame}");
    }

    #[test]
    fn the_legend_fits_the_pane_at_the_reference_size() {
        // The **last** line of `LEGEND`, which is what a `Paragraph` drops first and
        // drops without saying so. Asserting a middle line would pass with the legend
        // one, two or ten lines over its pane; asserting this one cannot.
        let frame = help(Screen::Mine, &Config::default());
        let last = LEGEND.last().copied().unwrap_or_default();
        assert!(
            frame.contains(last),
            "the legend overflows its pane: {frame}"
        );
        // Every mark the theme owns is explained, plus the one it does not.
        for glyph in ['✓', '~', '✗', '●', '—'] {
            assert!(frame.contains(glyph), "{glyph} is missing from the legend");
        }

        // The Keys pane has the same 21 rows and the same silent clip, and Upgrades
        // is its worst case: nine globals, a blank, a heading, its five contextual
        // bindings, a blank and the four Mining lines come to exactly 21. Checked on
        // the last Mining line for the same reason as above.
        let upgrades = help(Screen::Upgrades, &Config::default());
        let last = MINING.last().copied().unwrap_or_default();
        assert!(
            upgrades.contains(last),
            "the Keys pane overflows on the screen with the most bindings: {upgrades}"
        );
    }

    #[test]
    fn it_names_the_screen_it_was_opened_from() {
        // Opened from Inventory, it lists Inventory's keys and titles the block.
        let frame = help(Screen::Inventory, &Config::default());
        assert!(frame.contains("On this screen — Inventory"), "{frame}");
        assert!(frame.contains("compress"), "{frame}");
        // Not another screen's — the sub-tab line belongs to Upgrades only.
        assert!(!frame.contains("switch sub-tab"), "{frame}");
    }

    #[test]
    fn each_screen_gets_its_own_contextual_bindings() {
        // Opening Help over a screen lists that screen's keys, not another's.
        assert!(help(Screen::Mines, &Config::default()).contains("richness dial"));
        assert!(help(Screen::Stats, &Config::default()).contains("prestige"));
        assert!(help(Screen::Levels, &Config::default()).contains("jump to your level"));
    }

    #[test]
    fn the_mine_screen_has_no_contextual_block_only_mining() {
        // Mine's one key is Space, which lives under Mining; so no "On this screen".
        let frame = help(Screen::Mine, &Config::default());
        assert!(!frame.contains("On this screen"), "{frame}");
        assert!(frame.contains("hold to mine"), "{frame}");
    }

    #[test]
    fn the_sub_tab_line_is_rendered_from_config_not_the_default() {
        // The whole point of §6.11's dynamic line: change the binding, and Help
        // shows the chosen keys, never the default it was configured away from.
        let default = help(Screen::Upgrades, &Config::default());
        assert!(default.contains("⇧← ⇧→"), "{default}");
        assert!(default.contains("switch sub-tab"), "{default}");

        let rebound = Config {
            sub_tab_keys: SubTabKeys::HL,
        };
        let frame = help(Screen::Upgrades, &rebound);
        assert!(frame.contains("h  l"), "{frame}");
        assert!(!frame.contains("⇧← ⇧→"), "help showed the default: {frame}");
    }
}
