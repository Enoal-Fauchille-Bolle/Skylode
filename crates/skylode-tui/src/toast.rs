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
    style::Style,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::theme;

/// How long a toast stays up before [`Toasts::prune`] drops it.
pub const TOAST_TTL: Duration = Duration::from_secs(3);

/// A single announcement and the instant it stops being shown.
///
/// It stores an expiry rather than a remaining duration so that expiry is a
/// comparison against `now` — no countdown to decrement, and therefore nothing
/// that drifts if a tick is late or skipped.
#[derive(Clone, Debug)]
struct Toast {
    text: String,
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

    /// Queues an announcement for `ttl`.
    pub fn push(&mut self, text: String, ttl: Duration) {
        self.items.push(Toast {
            text,
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
        // The accent on the border, not the muted grey every other box takes: a
        // toast is the one overlay with no title to carry emphasis, and it is
        // announcing something that just happened. Muting it would hide the only
        // element on screen whose whole purpose is to be noticed within 3 seconds.
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme::ACCENT));
        frame.render_widget(
            Paragraph::new(toast.text.clone()).block(block).centered(),
            rect,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_queue_is_empty() {
        assert!(Toasts::new().items.is_empty());
    }

    #[test]
    fn pruning_drops_the_expired_and_keeps_the_fresh() {
        let mut toasts = Toasts::new();
        toasts.push("expired".to_owned(), Duration::ZERO);
        toasts.push("fresh".to_owned(), Duration::from_secs(60));

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
        toasts.push("older".to_owned(), TOAST_TTL);
        toasts.push("newer".to_owned(), TOAST_TTL);
        let newest = toasts.items.last().map(|toast| toast.text.as_str());
        assert_eq!(newest, Some("newer"));
    }
}
