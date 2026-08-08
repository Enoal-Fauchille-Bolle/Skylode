//! The Settings screen (UI.md §6.10).
//!
//! **Every config field, and no game-state field.** That rule is only auditable if
//! the list is short enough to read at a glance, and this frame is what makes the
//! audit possible: every line is a preference and nothing here is state. A setting
//! is what you add when a preference is genuinely contested — not a way of declining
//! to decide. Stored in the save, edited only here, which is what keeps the HMAC
//! quiet when a colour changes.
//!
//! Square borders (`┌┐`), the boot-and-modal class, not the screens' rounded ones.
//!
//! ## One renderer, two doors
//!
//! [`render`] takes a `&Config`, a [`SettingsRow`] and a [`Capabilities`] — plain data,
//! and deliberately
//! neither an `App` nor a `Session`. That is what lets the same frame answer both ways
//! in: from a running game it is a [`Modal`](super::Modal) stacked over the six tabs,
//! and from the title it is a screen the [`Splash`](crate::session::Splash) opens in
//! place of its menu. A renderer that reached for the run could only ever have served
//! the first, and the title has no run to give it.
//!
//! ## The rows are a closed enum, and their order is the struct's
//!
//! [`ROWS`] is the audit made mechanical: it walks the same five preferences
//! [`Config`] declares, in the same order, so *"is every config field on this screen"*
//! is answered by reading two lists side by side rather than by remembering. A sixth
//! preference that never reached this enum would show up as a `match` the compiler
//! refuses to accept.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Clear, Paragraph},
};

use super::square;
use crate::{
    capability::Capabilities,
    config::{Config, MiningInput, NumberFormat, SubTabKeys, ToastDuration},
    cursor::step_in,
    palette::ColourMode,
    theme,
};

/// Where the value column starts, counted from the wireframe.
///
/// `docs/UI.md` §6.10 draws ` ▸ Colour            256` — a two-column mark, then the
/// label, then the value at column 21 of the panel. Written once so a longer label
/// cannot push one row's value out of line with the four above it.
const VALUE_COLUMN: usize = 19;

/// The sentence the whole screen exists to make true, printed under every row.
///
/// **On every row rather than only on `Colour`**: it explains why editing here is safe,
/// and a player who never lands on the first row would otherwise never read it. Hoisted
/// out of [`SettingsRow::detail`] so that [`render`] can give it a style of its own —
/// the pane is one [`Paragraph`], which carries a single style, so the only way to mute
/// four lines out of twenty is for the lines to carry it themselves.
const FOOTNOTE: [&str; 4] = [
    " Stored in the save. There is no config",
    " file; Settings is the only way to",
    " change these — which is what keeps the",
    " HMAC quiet when you change a colour.",
];

/// One line of the Settings screen.
///
/// A closed enum walked through [`ROWS`], like every other cursor in this crate: a row
/// that is not there cannot be pointed at. It carries no data — the values are
/// [`Config`]'s, and a payload here would be a second copy of the preference that
/// could disagree with the one the game is actually reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsRow {
    /// How many colours to ask the terminal for.
    Colour,
    /// Whether the mine key is held or toggled.
    MiningInput,
    /// How a figure's thousands are punctuated.
    NumberFormat,
    /// How long an announcement stays up.
    ToastDuration,
    /// The Upgrades sub-tab switch binding.
    SubTabKeys,
}

/// The rows, in the order they are drawn — which is [`Config`]'s field order.
pub const ROWS: [SettingsRow; 5] = [
    SettingsRow::Colour,
    SettingsRow::MiningInput,
    SettingsRow::NumberFormat,
    SettingsRow::ToastDuration,
    SettingsRow::SubTabKeys,
];

impl SettingsRow {
    /// The row's name, as drawn in the left column.
    fn label(self) -> &'static str {
        match self {
            Self::Colour => "Colour",
            Self::MiningInput => "Mining input",
            Self::NumberFormat => "Number format",
            Self::ToastDuration => "Toast duration",
            Self::SubTabKeys => "Sub-tab keys",
        }
    }

    /// What `config` currently holds for this row.
    ///
    /// Every arm goes through the preference's own `label`, so the value column and
    /// the detail pane beside it cannot word the same choice two ways.
    fn value(self, config: &Config) -> &'static str {
        match self {
            Self::Colour => config.colour.label(),
            Self::MiningInput => config.mining_input.label(),
            Self::NumberFormat => config.number_format.label(),
            Self::ToastDuration => config.toast_duration.label(),
            Self::SubTabKeys => config.sub_tab_keys.label(),
        }
    }

    /// Turns the value under the cursor by `delta`, **wrapping** at either end.
    ///
    /// Through [`step_in`] rather than a hand-rolled modulo per row, which is what
    /// makes *every list wraps* one rule here as well: each preference is a closed set,
    /// so the ladder a `→` walks is the enum's own variants. A two-variant row like
    /// `Colour` therefore flips on either direction, which is the same answer the dev
    /// menu gives its toggle — a two-state row has no direction to respect.
    ///
    /// Takes `&mut Config` rather than returning a new one because the caller owns the
    /// preferences and is about to have them written to disk; handing back a copy would
    /// invite a call site that forgot to store it.
    pub fn adjust(self, config: &mut Config, delta: isize) {
        match self {
            Self::Colour => {
                config.colour = step_in(
                    &[ColourMode::Ansi256, ColourMode::Ansi16],
                    config.colour,
                    delta,
                );
            }
            Self::MiningInput => {
                config.mining_input = step_in(
                    &[MiningInput::Hold, MiningInput::PressToStart],
                    config.mining_input,
                    delta,
                );
            }
            Self::NumberFormat => {
                config.number_format = step_in(
                    &[
                        NumberFormat::Spaced,
                        NumberFormat::Comma,
                        NumberFormat::Plain,
                    ],
                    config.number_format,
                    delta,
                );
            }
            Self::ToastDuration => {
                config.toast_duration = step_in(
                    &[
                        ToastDuration::Brief,
                        ToastDuration::Normal,
                        ToastDuration::Long,
                        ToastDuration::Lingering,
                    ],
                    config.toast_duration,
                    delta,
                );
            }
            Self::SubTabKeys => {
                config.sub_tab_keys = step_in(
                    &[
                        SubTabKeys::ShiftArrows,
                        SubTabKeys::HL,
                        SubTabKeys::Brackets,
                    ],
                    config.sub_tab_keys,
                    delta,
                );
            }
        }
    }

    /// Puts **this row alone** back to what a fresh install would have shown (`r`).
    ///
    /// **Read out of [`Config::default`] rather than written down again.** Which value
    /// is the default is already declared once — in the `#[default]` attribute on each
    /// preference's enum — and a second copy here would be a second source of truth
    /// that can drift without anything failing. So the answer is fetched from the type
    /// that owns it, and this function only knows *which field* to copy across.
    ///
    /// **One row and never all five**, which is the whole of the design decision. This
    /// is the one destructive gesture on the one screen with no confirmation, and every
    /// ladder here is short enough to walk back by hand in at most three presses — so a
    /// *reset everything* would buy two keystrokes at the price of the only key on the
    /// screen that can undo work the player meant to keep.
    pub fn reset(self, config: &mut Config) {
        let fresh = Config::default();
        match self {
            Self::Colour => config.colour = fresh.colour,
            Self::MiningInput => config.mining_input = fresh.mining_input,
            Self::NumberFormat => config.number_format = fresh.number_format,
            Self::ToastDuration => config.toast_duration = fresh.toast_duration,
            Self::SubTabKeys => config.sub_tab_keys = fresh.sub_tab_keys,
        }
    }

    /// The next row down, wrapping — the tab ring's rule, applied to a five-row list.
    pub fn step(self, delta: isize) -> Self {
        step_in(&ROWS, self, delta)
    }

    /// What the pane on the right says about this row.
    ///
    /// **Every choice is spelled out, not just the one in force.** The value column
    /// shows what the preference *is*; this is where the player finds out what the
    /// other answers would do, which is the only reason a settings screen needs two
    /// panels rather than one list.
    ///
    /// It no longer opens with the row's name: that line became the pane's **title**,
    /// which is what gives it ACCENT and bold without this module naming a colour —
    /// `docs/UI.md` §4.4's rule is that colour doubles a glyph and never replaces one,
    /// and a block title is a route §4.5 already opened. Nor does it carry
    /// [`FOOTNOTE`], for the reason that constant documents.
    fn detail(self, caps: Capabilities) -> Vec<String> {
        match self {
            Self::Colour => vec![
                " 256   every block gets its own swatch".to_owned(),
                " 16    one colour per mine; the value".to_owned(),
                "       cell's stipple is what still".to_owned(),
                "       tells it from the common one".to_owned(),
                String::new(),
                // The one line in the pane that is a *fact about this machine* rather
                // than a description of a choice. It informs the decision and never
                // makes it — see `crate::capability`.
                format!(" Your terminal reports: {}", caps.colour_report()),
            ],
            // The wording avoids "toggle": the player is told what to *do*, and the
            // latch behind it is machinery they never have to picture. What they do
            // need is the one place the two modes differ in feel, which is the last
            // line — see `App::advance` for why the delay exists at all.
            Self::MiningInput => vec![
                " Hold            mine while the key is".to_owned(),
                "                 held down".to_owned(),
                " Press to start  one press starts, the".to_owned(),
                "                 next stops".to_owned(),
                String::new(),
                " Mining only happens on the Mine screen".to_owned(),
                " in both modes: leaving pauses it and".to_owned(),
                " coming back resumes it.".to_owned(),
                String::new(),
                " On a terminal that cannot report a key".to_owned(),
                " release, allow about a second between".to_owned(),
                " two presses.".to_owned(),
            ],
            Self::NumberFormat => vec![
                " How a figure's thousands are split.".to_owned(),
                String::new(),
                " Numbers are always exact — never".to_owned(),
                " 1.23M — so that a price and a pile".to_owned(),
                " can be compared in your head.".to_owned(),
            ],
            Self::ToastDuration => vec![
                " How long an announcement stays in the".to_owned(),
                " corner before it fades.".to_owned(),
                String::new(),
                " Nothing is lost either way: the Stats".to_owned(),
                " screen keeps the whole history for as".to_owned(),
                " long as the session lasts.".to_owned(),
            ],
            Self::SubTabKeys => vec![
                " Which key pair switches between the".to_owned(),
                " Upgrades sub-tabs.".to_owned(),
                String::new(),
                " ← and → are taken everywhere: they".to_owned(),
                " adjust whatever the cursor is on —".to_owned(),
                " this row included.".to_owned(),
                String::new(),
                " Help shows whichever pair you pick.".to_owned(),
            ],
        }
    }
}

/// Draws the Settings screen: the five rows, the detail of the selected one, a footer.
///
/// [`Clear`] first, like [`help`](super::help): both are full-frame overlays, and the
/// panels only paint the cells their text occupies — without it the screen underneath
/// shows through wherever a row is short.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    config: &Config,
    row: SettingsRow,
    caps: Capabilities,
) {
    let [body, footer_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    // `38 : 42`, the counted split, reused as the `Fill` ratio so the frame is
    // unchanged at 80 columns and shares the surplus above it.
    let [left, right] =
        Layout::horizontal([Constraint::Fill(38), Constraint::Fill(42)]).areas(body);

    let fields: Vec<String> = ROWS
        .iter()
        .map(|&entry| {
            // The `▸` is a mark, so `theme::marked` below gives it ACCENT without this
            // module naming a colour — the route every list row in the crate takes.
            let mark = if entry == row { "▸" } else { " " };
            let label = entry.label();
            let padding = VALUE_COLUMN.saturating_sub(label.chars().count());
            format!(
                " {mark} {label}{:padding$}{}",
                "",
                entry.value(config),
                padding = padding
            )
        })
        .collect();

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(
            fields
                .iter()
                .map(|line| theme::marked(line))
                .collect::<Vec<_>>(),
        )
        .block(square(" Settings ")),
        left,
    );
    // The choices first, then a blank, then the footnote — the same three parts the
    // pane always had, assembled here rather than inside `detail` so the last part can
    // be muted. `MUTED` carries no meaning anywhere in this crate: it is the *less
    // urgent* treatment the footers, the table headers and `marked_row`'s value column
    // already wear, so stepping this paragraph back makes no information depend on a
    // hue. `docs/UI.md` §4.4.
    let mut pane: Vec<Line> = row.detail(caps).into_iter().map(Line::from).collect();
    pane.push(Line::default());
    pane.extend(FOOTNOTE.map(|text| Line::styled(text, Style::default().fg(theme::MUTED))));
    frame.render_widget(
        // The row's name is the pane's title, which is where its ACCENT comes from —
        // see `detail`. Spaced on both sides like every other block title in the crate.
        Paragraph::new(pane).block(square(&format!(" {} ", row.label()))),
        right,
    );

    // Muted, like every other footer in the crate. It prints `r` because this screen is
    // the only place the key means anything, and an undo nobody can see is an undo
    // nobody uses.
    frame.render_widget(
        Paragraph::new(" ↑↓  select     ← →  change     r  default     Esc  back")
            .style(Style::default().fg(theme::MUTED)),
        footer_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders `config` with `row` selected, as one frame string.
    fn frame_of(config: &Config, row: SettingsRow) -> String {
        crate::overlay::render_to_string(|frame, area| {
            render(
                frame,
                area,
                config,
                row,
                Capabilities::with_colours(Some(256)),
            );
        })
    }

    /// The same draw, kept as cells so a test can ask what hue something was drawn in.
    fn buffer_of(config: &Config, row: SettingsRow) -> ratatui::buffer::Buffer {
        crate::overlay::render_to_buffer(80, 24, |frame, area| {
            render(
                frame,
                area,
                config,
                row,
                Capabilities::with_colours(Some(256)),
            );
        })
    }

    #[test]
    fn it_lists_only_preferences_with_the_colour_detail() {
        let frame = frame_of(&Config::default(), SettingsRow::Colour);
        assert!(frame.contains("Colour             256"), "{frame}");
        assert!(frame.contains("Mining input       Hold"), "{frame}");
        assert!(frame.contains("Sub-tab keys"), "{frame}");
        // The detail pane explains why editing here does not upset the HMAC.
        assert!(frame.contains("keeps the"), "{frame}");
        assert!(frame.contains("Esc  back"), "{frame}");
    }

    /// **The audit, run by a test rather than by a reader.**
    ///
    /// `docs/UI.md` §6.10's rule is *every config field, and no game-state field*, and
    /// its first half is only true as long as nobody adds a preference without adding a
    /// row. Walking [`ROWS`] and asserting each label reaches the frame is what turns
    /// that from a habit into a failure.
    #[test]
    fn every_row_of_the_enum_reaches_the_screen() {
        let frame = frame_of(&Config::default(), SettingsRow::Colour);
        for row in ROWS {
            assert!(
                frame.contains(row.label()),
                "{} is missing: {frame}",
                row.label()
            );
        }
    }

    #[test]
    fn the_caret_marks_the_selected_row_and_only_that_row() {
        let frame = frame_of(&Config::default(), SettingsRow::NumberFormat);
        assert_eq!(frame.matches('▸').count(), 1, "{frame}");
        let marked = frame
            .lines()
            .find(|line| line.contains('▸'))
            .unwrap_or_default();
        assert!(marked.contains("Number format"), "{frame}");
    }

    /// The pane follows the caret, and each row has something of its own to say.
    #[test]
    fn the_detail_pane_describes_whichever_row_is_selected() {
        let mining = frame_of(&Config::default(), SettingsRow::MiningInput);
        assert!(mining.contains("one press starts"), "{mining}");
        assert!(
            mining.contains("only happens on the Mine screen"),
            "{mining}"
        );

        let toasts = frame_of(&Config::default(), SettingsRow::ToastDuration);
        assert!(toasts.contains("keeps the whole history"), "{toasts}");
        assert!(
            !toasts.contains("one press starts"),
            "the pane kept the previous row's text: {toasts}"
        );
    }

    /// The one line in the pane that is a fact about the machine rather than a choice.
    #[test]
    fn the_colour_pane_quotes_what_the_terminal_declared() {
        let frame = crate::overlay::render_to_string(|frame, area| {
            render(
                frame,
                area,
                &Config::default(),
                SettingsRow::Colour,
                Capabilities::with_colours(None),
            );
        });
        assert!(
            frame.contains("Your terminal reports: nothing at all"),
            "{frame}"
        );
    }

    /// The value column tracks the config, which is what makes the screen an editor
    /// rather than a picture. Asserted on a config dialled away from every default, so
    /// a row still rendering `Config::default()` could not pass.
    #[test]
    fn the_value_column_reads_the_config_it_was_handed() {
        let dialled = Config {
            colour: ColourMode::Ansi16,
            mining_input: MiningInput::PressToStart,
            number_format: NumberFormat::Comma,
            toast_duration: ToastDuration::Lingering,
            sub_tab_keys: SubTabKeys::Brackets,
        };
        let frame = frame_of(&dialled, SettingsRow::Colour);
        assert!(frame.contains("Colour             16"), "{frame}");
        assert!(frame.contains("Press to start"), "{frame}");
        assert!(frame.contains("1,234,567"), "{frame}");
        assert!(frame.contains("8s"), "{frame}");
        assert!(frame.contains("[  ]"), "{frame}");
    }

    /// Every row's ladder wraps, in both directions — the crate's one list rule.
    ///
    /// Walked as a loop rather than spelled per row, because what is under test is that
    /// **no** row was given a clamp of its own: a single hand-rolled arm would be
    /// exactly the drift this catches.
    #[test]
    fn every_row_wraps_rather_than_stopping_at_either_end() {
        for row in ROWS {
            let mut forward = Config::default();
            let mut laps = 0;
            loop {
                row.adjust(&mut forward, 1);
                laps += 1;
                if forward == Config::default() {
                    break;
                }
                assert!(laps < 16, "{} never came back round", row.label());
            }
            // And the other way lands on the same place after the same number of steps.
            let mut backward = Config::default();
            for _ in 0..laps {
                row.adjust(&mut backward, -1);
            }
            assert_eq!(
                backward,
                Config::default(),
                "{} did not wrap back",
                row.label()
            );
        }
    }

    /// A row turns **its own** preference and leaves the other four alone.
    ///
    /// The defect this guards against is a copy-paste in `adjust` — five nearly
    /// identical arms — where one row writes the field above it.
    #[test]
    fn adjusting_a_row_touches_no_other_preference() {
        for row in ROWS {
            let mut config = Config::default();
            row.adjust(&mut config, 1);
            let changed = ROWS
                .iter()
                .filter(|&&other| other.value(&config) != other.value(&Config::default()))
                .count();
            assert_eq!(changed, 1, "{} changed {changed} rows", row.label());
        }
    }

    /// `r` restores **the row under the caret**, and only that one.
    ///
    /// Run from a config dialled away from every default, so the two halves of the
    /// claim are tested at once by counting: after one reset, exactly one of the five
    /// rows should read what a fresh install reads. A `reset` that wrote the whole
    /// struct — the obvious mistake, since it already builds a `Config::default()` —
    /// would restore five and fail here rather than quietly throwing away four
    /// preferences the player had chosen.
    #[test]
    fn r_restores_one_row_and_leaves_the_other_four_alone() {
        let dialled = Config {
            colour: ColourMode::Ansi16,
            mining_input: MiningInput::PressToStart,
            number_format: NumberFormat::Comma,
            toast_duration: ToastDuration::Lingering,
            sub_tab_keys: SubTabKeys::Brackets,
        };
        let fresh = Config::default();

        for row in ROWS {
            let mut config = dialled.clone();
            row.reset(&mut config);
            assert_eq!(
                row.value(&config),
                row.value(&fresh),
                "{} did not go back to its default",
                row.label()
            );
            let restored = ROWS
                .iter()
                .filter(|&&other| other.value(&config) == other.value(&fresh))
                .count();
            assert_eq!(restored, 1, "{} restored {restored} rows", row.label());
        }
    }

    /// Restoring a row that is already default changes nothing — `r` is the one
    /// destructive key on a screen with no confirmation, so pressing it twice must not
    /// be a second act.
    #[test]
    fn restoring_an_untouched_row_is_not_a_change() {
        let mut config = Config::default();
        for row in ROWS {
            row.reset(&mut config);
        }
        assert_eq!(config, Config::default());
    }

    /// The key is printed, because an undo nobody can see is an undo nobody uses.
    #[test]
    fn the_footer_advertises_the_restore_key() {
        let frame = frame_of(&Config::default(), SettingsRow::Colour);
        assert!(frame.contains("r  default"), "{frame}");
    }

    /// **The row's name is the pane's title**, which is where its colour comes from.
    ///
    /// `docs/UI.md` §4.4 says a hue doubles a glyph and never replaces one, so the only
    /// admissible way to accent this line was to make it a *thing that is already
    /// accented* — a block title — rather than to paint a body line. The colour is
    /// therefore asserted together with the position: on the top border row, which is
    /// the only place a title can be, and in ACCENT, which is what `square` gives every
    /// title in the crate without this module naming a colour.
    #[test]
    fn the_detail_pane_is_titled_with_the_row_it_describes() {
        let frame = frame_of(&Config::default(), SettingsRow::NumberFormat);
        let top = frame.lines().next().unwrap_or_default();
        assert!(top.contains("Number format"), "{frame}");

        // The first `N` in the frame scanning downwards is that title's: the row of the
        // same name is four lines below it in the left column.
        let buffer = buffer_of(&Config::default(), SettingsRow::NumberFormat);
        assert_eq!(crate::overlay::colour_of(&buffer, "N"), Some(theme::ACCENT));
    }

    /// The footnote steps back; the choices above it do not.
    ///
    /// **`MUTED` is the one colour in the palette that carries no meaning** — it is the
    /// *less urgent* treatment already worn by every footer, every table header and
    /// `marked_row`'s value column — so muting this paragraph makes no information
    /// depend on a hue, which is the test's real claim. The second assertion is what
    /// makes it a *limit* rather than a licence: the pane's choices keep the terminal's
    /// own foreground, and a pass that muted the whole pane would fail here.
    #[test]
    fn the_footnote_is_muted_and_the_choices_above_it_are_not() {
        let buffer = buffer_of(&Config::default(), SettingsRow::Colour);
        // The em dash appears once on this row's pane, in the footnote's third line.
        assert_eq!(crate::overlay::colour_of(&buffer, "—"), Some(theme::MUTED));
        // And `w` appears first in the pane's own body — `own swatch` — since no label,
        // value or footer on this frame carries the letter.
        assert_eq!(
            crate::overlay::colour_of(&buffer, "w"),
            Some(ratatui::style::Color::Reset),
            "the choices were muted along with the footnote"
        );
    }

    #[test]
    fn the_row_cursor_wraps_at_both_ends() {
        assert_eq!(SettingsRow::Colour.step(-1), SettingsRow::SubTabKeys);
        assert_eq!(SettingsRow::SubTabKeys.step(1), SettingsRow::Colour);
        assert_eq!(SettingsRow::Colour.step(1), SettingsRow::MiningInput);
    }
}
