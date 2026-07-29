//! Ephemeral announcements drawn over the current screen.
//!
//! UI-EN.md §3.3: no screen owns announcements, and six specified mechanics need
//! one. The design settles them as a 2–3 s overlay at the bottom of whatever
//! screen you are on, with the full history kept in Stats — *one buffer, two
//! renderings*, which is what makes "ephemeral plus history" a single feature.
//!
//! The toast costs **zero permanent layout rows**: it is drawn over the frame
//! with [`Clear`], not laid out beside anything, so adding it never moved the
//! mine grid. That is why the Mine screen's budget still closes at 24 rows.
//!
//! Today toasts are pushed by a demo key. Once `tick()` returns `Vec<Event>`
//! (phase 7), this queue becomes the tail of that stream and stops being driven
//! by input at all.

use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::theme;

/// How long a toast stays up before [`Toasts::prune`] drops it.
pub const TOAST_TTL: Duration = Duration::from_secs(3);

/// What kind of news a toast carries, and therefore what colour it is drawn in.
///
/// **The words still carry the answer alone.** §4.4's redundancy rule holds here in
/// the form it can: a toast has no glyph to double, so the hue doubles the *sentence* —
/// remove every colour and `Not enough Stone` still says what `Bought Netherite
/// Pickaxe` does not. What the colour buys is the three seconds: a refusal and a
/// purchase were drawn identically, so the one announcement in the interface that
/// exists to be noticed inside its own lifetime looked exactly like the one that does
/// not need to be.
///
/// **No `Default`, and `push` takes it rather than a second method taking it.** A
/// refusal that quietly fell back to neutral is the bug this fixes; making every one of
/// the call sites in [`app`](crate::app) name its news is what stops that regressing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Something happened and nothing is at stake: a dial moved, a mine was entered.
    Neutral,
    /// A purchase went through, or a conversion did.
    Success,
    /// The ore is not there — the loop that fixes it is a trip to a mine.
    Refusal,
    /// The value is held and the denomination is not — the loop is `3 Inventory`.
    CompressFirst,
}

impl Tone {
    /// The hue, from [`theme`]'s own table rather than from a second one here.
    ///
    /// The three that name a verdict take the same colours their `✓ ~ ✗` take on the
    /// Upgrades screen, so a toast and the mark that predicted it cannot disagree.
    /// `Neutral` keeps the accent the border has always had.
    fn colour(self) -> Color {
        match self {
            Self::Neutral => theme::ACCENT,
            Self::Success => theme::AFFORDABLE,
            Self::Refusal => theme::REFUSED,
            Self::CompressFirst => theme::COMPRESS_FIRST,
        }
    }
}

/// A single announcement, the news it carries, and the instant it stops being shown.
///
/// It stores an expiry rather than a remaining duration so that expiry is a
/// comparison against `now` — no countdown to decrement, and therefore nothing
/// that drifts if a tick is late or skipped.
#[derive(Clone, Debug)]
struct Toast {
    text: String,
    tone: Tone,
    expires_at: Instant,
}

/// The live toast queue.
#[derive(Clone, Debug, Default)]
pub struct Toasts {
    items: Vec<Toast>,
}

impl Toasts {
    /// An empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many toasts are currently live.
    ///
    /// Only tests read this today. It is kept rather than deleted because the
    /// Stats history panel (UI-EN.md §5.6) reads the same buffer, and the
    /// `cfg_attr` says so out loud instead of letting a silent `allow` hide it.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "awaiting the Stats history panel")
    )]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing is showing.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "awaiting the Stats history panel")
    )]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Queues an announcement of `tone` for `ttl`.
    pub fn push(&mut self, text: String, tone: Tone, ttl: Duration) {
        self.items.push(Toast {
            text,
            tone,
            expires_at: Instant::now() + ttl,
        });
    }

    /// Drops every toast whose moment has passed.
    ///
    /// `now` is a parameter rather than an internal `Instant::now()` call for the
    /// same reason the core makes the caller inject time: a function that reads
    /// the clock itself cannot be tested, and expiry is exactly the behaviour
    /// worth testing here.
    pub fn prune(&mut self, now: Instant) {
        self.items.retain(|toast| toast.expires_at > now);
    }

    /// Draws the most recent toast near the bottom of `area`, over whatever is
    /// already there.
    ///
    /// Only the newest is shown: stacking them would reintroduce the layout cost
    /// the overlay exists to avoid, and the full record is Stats' job anyway.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let Some(toast) = self.items.last() else {
            return;
        };

        // Two cells of padding either side of the text, plus the two borders.
        let text_width = u16::try_from(toast.text.chars().count()).unwrap_or(u16::MAX);
        let width = text_width.saturating_add(4).min(area.width);
        let height = 3.min(area.height);
        // Sits one row above the bottom edge, leaving the footer line clear.
        let y = area.y + area.height.saturating_sub(height + 1);
        let rect = Rect {
            x: area.x + area.width.saturating_sub(width) / 2,
            y,
            width,
            height,
        };

        // Clear first: without it the panel underneath bleeds through the gaps.
        frame.render_widget(Clear, rect);
        // Never the muted grey every other box takes: a toast is the one overlay with
        // no title to carry emphasis, and it is announcing something that just
        // happened. Muting it would hide the only element on screen whose whole purpose
        // is to be noticed within 3 seconds. Which hue it takes is the tone's, so a
        // refusal and a purchase stop looking alike.
        let style = Style::default().fg(toast.tone.colour());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(style);
        frame.render_widget(
            Paragraph::new(toast.text.clone())
                .block(block)
                .style(style)
                .centered(),
            rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn a_fresh_queue_is_empty() {
        assert!(Toasts::new().items.is_empty());
    }

    #[test]
    fn pruning_drops_the_expired_and_keeps_the_fresh() {
        let mut toasts = Toasts::new();
        toasts.push("expired".to_owned(), Tone::Neutral, Duration::ZERO);
        toasts.push("fresh".to_owned(), Tone::Neutral, Duration::from_secs(60));

        toasts.prune(Instant::now());

        assert_eq!(toasts.items.len(), 1);
        assert_eq!(toasts.items[0].text, "fresh");
    }

    #[test]
    fn pruning_an_empty_queue_is_harmless() {
        let mut toasts = Toasts::new();
        toasts.prune(Instant::now());
        assert!(toasts.items.is_empty());
    }

    #[test]
    fn the_newest_toast_is_the_one_rendered() {
        let mut toasts = Toasts::new();
        toasts.push("older".to_owned(), Tone::Neutral, TOAST_TTL);
        toasts.push("newer".to_owned(), Tone::Neutral, TOAST_TTL);
        let newest = toasts.items.last().map(|toast| toast.text.as_str());
        assert_eq!(newest, Some("newer"));
    }

    #[test]
    fn every_tone_takes_a_colour_from_the_theme_and_no_two_verdicts_share_one() {
        // The three that name a verdict must separate from each other, or the colour
        // stops being a reading and becomes decoration. They are asserted against
        // `theme`'s own constants rather than against literals, so a re-theme moves the
        // toast with the `✓ ~ ✗` it doubles instead of leaving it behind.
        assert_eq!(Tone::Neutral.colour(), theme::ACCENT);
        assert_eq!(Tone::Success.colour(), theme::AFFORDABLE);
        assert_eq!(Tone::Refusal.colour(), theme::REFUSED);
        assert_eq!(Tone::CompressFirst.colour(), theme::COMPRESS_FIRST);

        let verdicts = [Tone::Success, Tone::Refusal, Tone::CompressFirst];
        for (index, tone) in verdicts.iter().enumerate() {
            for other in &verdicts[index + 1..] {
                assert_ne!(tone.colour(), other.colour(), "{tone:?} and {other:?}");
            }
        }
    }

    #[test]
    fn a_toast_is_drawn_in_its_own_tone_border_and_text_alike() {
        // The defect: a refusal and a purchase were the same picture, so the one
        // announcement with three seconds to be noticed looked like the one that has
        // nothing at stake.
        for (tone, expected) in [
            (Tone::Refusal, theme::REFUSED),
            (Tone::Success, theme::AFFORDABLE),
        ] {
            let mut toasts = Toasts::new();
            toasts.push("news".to_owned(), tone, TOAST_TTL);

            let mut terminal = match Terminal::new(TestBackend::new(20, 5)) {
                Ok(terminal) => terminal,
                Err(infallible) => match infallible {},
            };
            if let Err(infallible) = terminal.draw(|frame| {
                toasts.render(frame, frame.area());
            }) {
                match infallible {}
            }
            let buffer = terminal.backend().buffer().clone();

            let found = |needle: &str| {
                (0..buffer.area.height).find_map(|y| {
                    (0..buffer.area.width)
                        .find(|&x| buffer[(x, y)].symbol() == needle)
                        .map(|x| buffer[(x, y)].fg)
                })
            };
            assert_eq!(found("╭"), Some(expected), "{tone:?}'s border");
            assert_eq!(found("n"), Some(expected), "{tone:?}'s text");
        }
    }
}
