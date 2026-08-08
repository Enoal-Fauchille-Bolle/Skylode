//! What the terminal says about itself — read once, at startup.
//!
//! One question so far: **how many colours**. The Settings screen prints the answer
//! beside the `Colour` row (`docs/UI.md` §6.10) so that a player choosing between 256
//! and 16 is choosing against a fact rather than against a guess.
//!
//! ## It reports; it never decides
//!
//! Nothing here overrides [`ColourMode`](crate::palette::ColourMode). A terminal that
//! under-declares its palette is common — `TERM` is set by whatever launched the
//! shell, not by the emulator drawing the pixels — so a detection allowed to *force*
//! 16 colours would take the good palette away from a player who can see it. And the
//! converse matters too: a player who finds twenty-four swatches hard to tell apart
//! must be able to ask for the sixteen-colour rendering on a terminal that offers
//! more. The preference is the authority; this is a hint printed next to it.
//!
//! ## Why it is read in `main` and carried, rather than asked at the draw
//!
//! [`detect`](Capabilities::detect) reads the environment, and the environment is
//! ambient state: a renderer that consulted it would draw differently depending on
//! which shell ran the test suite. Read once at the edge and passed down, the value is
//! an ordinary argument every test can name — the same treatment `main` already gives
//! the seed and `SKYLODE_DEV`.

use std::env;

/// The terminal facts the interface is allowed to mention.
///
/// [`Copy`] and two words wide, so it rides along beside the config without anyone
/// having to think about borrowing it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// How many colours the environment claims, or [`None`] when it claims nothing.
    ///
    /// **An [`Option`] and not a floor of 16**, because *"the terminal says nothing"*
    /// and *"the terminal says sixteen"* are different sentences and the screen prints
    /// them differently. Defaulting the unknown case to 16 would put a specific,
    /// possibly wrong number in front of a player about to make a decision with it.
    colours: Option<u16>,
}

impl Capabilities {
    /// Asks the environment what it is willing to say.
    ///
    /// **Two variables and one answer, which is why they are `||`-ed rather than
    /// ranked.** `COLORTERM` is what an emulator sets about *itself* and `TERM` is what
    /// it was launched as — frequently through an `ssh` or a `tmux` that knows less than
    /// the thing drawing the pixels — so either one declaring a wide palette is enough,
    /// and neither is evidence *against* the other. A precedence between them would be
    /// machinery for a disagreement that cannot arise: this function has only two
    /// answers to give.
    ///
    /// The truecolour case is reported as 256 rather than as sixteen million, for the
    /// same reason. The only choice this number informs is a two-way one, and the
    /// palette the game draws with tops out at 256 (`crate::palette`); a figure the
    /// interface cannot act on would be trivia in the middle of a decision.
    pub fn detect() -> Self {
        let wide = matches!(env::var("COLORTERM").as_deref(), Ok("truecolor" | "24bit"))
            || env::var("TERM").is_ok_and(|term| term.contains("256color"));
        Self {
            colours: wide.then_some(256),
        }
    }

    /// What the Settings screen prints after *"Your terminal reports:"*.
    ///
    /// A whole clause and not a number, so the unknown case reads as a sentence
    /// instead of as a blank where a figure should be. Kept short because the pane it
    /// lands in is forty columns wide and the label ahead of it takes twenty-four:
    /// a longer clause is not wrapped by the renderer, it is **cut**.
    pub fn colour_report(self) -> String {
        match self.colours {
            Some(count) => format!("{count} supported"),
            None => "nothing at all".to_owned(),
        }
    }

    /// The same facts, stated rather than detected — for the tests, and for
    /// [`Default`], which is what a session gets when nobody asked.
    #[cfg(test)]
    pub fn with_colours(colours: Option<u16>) -> Self {
        Self { colours }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_that_declares_a_palette_has_it_quoted_back() {
        assert_eq!(
            Capabilities::with_colours(Some(256)).colour_report(),
            "256 supported"
        );
    }

    /// The case the [`Option`] exists for: silence is reported as silence.
    ///
    /// A floor of 16 here would read as *"your terminal reports: 16 supported"* on a
    /// perfectly capable emulator whose `TERM` was inherited through `ssh` — a
    /// confident wrong answer in the one place the player is making a decision.
    #[test]
    fn a_terminal_that_declares_nothing_is_not_given_a_number_on_its_behalf() {
        let quiet = Capabilities::with_colours(None);
        assert_eq!(quiet.colour_report(), "nothing at all");
        assert_eq!(quiet, Capabilities::default());
    }

    /// `detect` reads whatever this machine happens to export, so what is assertable
    /// about it is its *shape*: it answers, it does not panic, and the answer is one
    /// this crate can draw.
    #[test]
    fn detection_answers_something_the_screen_can_print() {
        let report = Capabilities::detect().colour_report();
        assert!(!report.is_empty());
    }
}
