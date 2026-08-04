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
//! **Toasts are the tick's, not the keyboard's.** Every `GameEvent` a simulation step
//! returns is worded by [`crate::announce`] and queued here; the only announcements
//! left that a keypress raises are the refusals, which no step can produce because
//! nothing was asked of the rules.
//!
//! **Nothing is ever dropped for being old, and that is the other half of *one buffer,
//! two renderings*.** This used to be a queue that forgot: a toast past its three
//! seconds was removed, which left the Stats history nothing to read. Expiry is now a
//! **question asked at draw time** rather than an edit made to the buffer — [`render`]
//! shows the newest announcement still inside its window and the log keeps every one
//! of them. So there is no second buffer to keep in step, and no moment where the two
//! renderings could disagree about what was said.
//!
//! The only thing that removes an entry is [`HISTORY_CAP`], and it removes the
//! **oldest** — which is why [`log`] hands its entries out newest-first: a rank
//! counted from the newest is one that dropping the oldest cannot move.
//!
//! [`render`]: Toasts::render
//! [`log`]: Toasts::log

use std::{
    collections::VecDeque,
    rc::Rc,
    time::{Duration, Instant},
};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use crate::theme;

/// How long a toast stays on screen before [`Toasts::render`] stops drawing it.
///
/// It bounds the *drawing* and not the *keeping*: the announcement stays in the buffer
/// for the Stats history either way.
pub const TOAST_TTL: Duration = Duration::from_secs(3);

/// How many announcements the buffer keeps before the oldest starts falling off.
///
/// **A count and not an age**, because this buffer is scrolled: an age cap would take
/// a line out from under the player's cursor while they were reading it, which is the
/// one thing a log must not do.
///
/// The number is bounded by the *copy*, not by the memory. [`View`](crate::view::View)
/// is rebuilt on every change of state — twenty times a second while mining — so the
/// cap is how much work a reprojection does. Five hundred entries is around eighteen
/// screens of scrollback at 80×24, and it is affordable only because an entry is an
/// [`Rc<str>`]: cloning one bumps a reference count instead of copying the sentence.
pub const HISTORY_CAP: usize = 500;

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

/// A single announcement, the news it carries, and the two instants that bound it.
///
/// It stores an expiry rather than a remaining duration so that expiry is a
/// comparison against `now` — no countdown to decrement, and therefore nothing
/// that drifts if a tick is late or skipped.
///
/// **[`at`](Toast::at) is kept beside it rather than derived**, even though the two
/// differ by a `ttl` the caller passed: the history prints how long ago something was
/// said, and subtracting a TTL that the *call site* chose would make the age of an
/// entry depend on how long its toast happened to be shown for.
///
/// The text is an [`Rc<str>`] — a **reference-counted pointer**, so cloning one
/// increments a counter instead of copying the sentence. It is what makes handing the
/// whole log to the read model cost five hundred increments and no allocation. [`Rc`]
/// and not [`Arc`](std::sync::Arc) because nothing here crosses a thread: the event
/// thread sends [`Event`](crate::event::Event)s and never touches this, so an atomic
/// counter would be paid for a guarantee no one needs.
#[derive(Clone, Debug)]
struct Toast {
    text: Rc<str>,
    tone: Tone,
    at: Instant,
    expires_at: Instant,
}

/// Every announcement this session has made, oldest first.
///
/// A [`VecDeque`] and not a [`Vec`], for the one operation the cap needs: dropping the
/// oldest is a `pop_front`, which a `Vec` answers by shifting every element it holds.
#[derive(Clone, Debug, Default)]
pub struct Toasts {
    items: VecDeque<Toast>,
}

impl Toasts {
    /// An empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many announcements this session has made.
    ///
    /// **It counts the log, not the screen**, and that is a change of meaning rather
    /// than of implementation: this buffer used to drop a toast when it expired, so
    /// the same call answered "how many are showing". Nothing is dropped now, so what
    /// is left to count is the record. The question "what is on screen" has exactly
    /// one caller — [`render`](Toasts::render) — and it needs an instant to answer.
    ///
    /// **It is what bounds the history cursor**, which is the caller that matters:
    /// `↑↓` on the Stats screen steps an index that has to wrap somewhere, and the
    /// buffer is the only thing that knows how many rows there are — the reducer has
    /// no view and no geometry. A dozen tests read it besides, for *how many
    /// announcements did this action raise*.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing has been announced at all.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "only tests count announcements; the panel reads `log`"
        )
    )]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Queues an announcement of `tone`, shown for `ttl`, starting now.
    ///
    /// **The one method here that reads a clock**, and it exists for the callers that
    /// have none: [`App::update`](crate::app::App::update) is a pure reducer over
    /// `(state, Action)` and must stay one, so a refusal it raises cannot be handed an
    /// instant. Everything on the tick's side goes through
    /// [`push_at`](Toasts::push_at) instead, where the instant is already in hand and
    /// a second reading of the clock could disagree with the one the step ran against.
    pub fn push(&mut self, text: String, tone: Tone, ttl: Duration) {
        self.push_at(text, tone, ttl, Instant::now());
    }

    /// The same, told what time it is.
    ///
    /// The **only** place the buffer ever shrinks, and it shrinks from the front. An
    /// announcement is never removed for being old — that is the whole of *one buffer,
    /// two renderings* — so the cap is what bounds it instead, and dropping the oldest
    /// is what keeps [`log`](Toasts::log)'s newest-first ranks stable under it.
    pub fn push_at(&mut self, text: String, tone: Tone, ttl: Duration, now: Instant) {
        self.items.push_back(Toast {
            text: Rc::from(text),
            tone,
            at: now,
            expires_at: now + ttl,
        });
        while self.items.len() > HISTORY_CAP {
            self.items.pop_front();
        }
    }

    /// Every announcement made this session, **newest first**, each with its age in
    /// whole seconds.
    ///
    /// **The age is a number and the text is shared**, which is the same rule
    /// [`PrestigeView`](crate::view::PrestigeView) states for itself: the read model
    /// carries values, and whatever draws them makes the sentence. Formatting `"2m"`
    /// here would spend an allocation per entry per reprojection on rows a terminal
    /// mostly cannot show, where the screen formats only the handful in its window.
    ///
    /// `now` is a parameter for the reason every clock in this crate is one: an age
    /// computed from an instant read inside would be untestable, and the caller
    /// already holds the instant its frame belongs to.
    pub fn log(&self, now: Instant) -> impl Iterator<Item = (u64, Rc<str>)> + '_ {
        self.items.iter().rev().map(move |toast| {
            (
                now.saturating_duration_since(toast.at).as_secs(),
                Rc::clone(&toast.text),
            )
        })
    }

    /// The announcement [`render`](Toasts::render) would draw, if there is one.
    ///
    /// **A search backwards** rather than a look at the last entry, because `ttl` is
    /// the caller's to choose: a short-lived announcement pushed after a long-lived one
    /// would otherwise blank the screen while something was still owed its seconds.
    fn current(&self, now: Instant) -> Option<&Toast> {
        self.items.iter().rev().find(|toast| toast.expires_at > now)
    }

    /// Whether an announcement is occupying its slot at `now`.
    ///
    /// **Asked by whatever else wants that slot**, which today is the standing
    /// save-failure banner in [`App::render`](crate::app::App::render). It shares
    /// [`current`](Toasts::current) with [`render`](Toasts::render) rather than
    /// re-deriving the rule, so "is a toast up" and "which toast is drawn" cannot come
    /// apart — a banner painted over a toast that was in fact showing is exactly the
    /// disagreement two copies of that search would allow.
    pub fn showing(&self, now: Instant) -> bool {
        self.current(now).is_some()
    }

    /// Draws the newest announcement still inside its window near the bottom of
    /// `area`, over whatever is already there.
    ///
    /// Only one is shown: stacking them would reintroduce the layout cost the overlay
    /// exists to avoid, and the full record is Stats' job anyway.
    ///
    /// **This is where a toast expires, and expiring is no longer something that
    /// happens *to* the buffer.** A `prune` used to delete the entry, which is why the
    /// history had nothing to read; the comparison lives here instead, so the same
    /// `expires_at` answers "is this still on screen" without anything being thrown
    /// away. `now` is a parameter for the reason it was one on `prune`: a function that
    /// reads the clock itself cannot be told what time it is, and expiry is exactly the
    /// behaviour worth testing.
    pub fn render(&self, frame: &mut Frame, area: Rect, now: Instant) {
        let Some(toast) = self.current(now) else {
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
            Paragraph::new(toast.text.to_string())
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

    /// Renders the queue into a small buffer and returns every row, joined.
    fn drawn(toasts: &Toasts, now: Instant) -> String {
        let mut terminal = match Terminal::new(TestBackend::new(30, 5)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        if let Err(infallible) = terminal.draw(|frame| {
            toasts.render(frame, frame.area(), now);
        }) {
            match infallible {}
        }
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// **The change this module exists to make.** An announcement past its three
    /// seconds leaves the screen and stays in the buffer — that is *one buffer, two
    /// renderings*, and the defect it fixes is a History panel with nothing to show.
    #[test]
    fn an_expired_announcement_leaves_the_screen_and_stays_in_the_log() {
        let now = Instant::now();
        let mut toasts = Toasts::new();
        toasts.push_at("expired".to_owned(), Tone::Neutral, Duration::ZERO, now);

        assert!(!drawn(&toasts, now).contains("expired"));
        let kept: Vec<Rc<str>> = toasts.log(now).map(|(_, text)| text).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(&*kept[0], "expired");
    }

    /// A short-lived announcement pushed after a long-lived one must not blank the
    /// screen: `ttl` is the caller's to choose, so the newest is not always the one
    /// with time left.
    #[test]
    fn the_newest_announcement_still_inside_its_window_is_the_one_drawn() {
        let now = Instant::now();
        let mut toasts = Toasts::new();
        toasts.push_at(
            "lasting".to_owned(),
            Tone::Neutral,
            Duration::from_secs(60),
            now,
        );
        toasts.push_at("gone".to_owned(), Tone::Neutral, Duration::ZERO, now);

        let frame = drawn(&toasts, now);
        assert!(frame.contains("lasting"), "{frame}");
        assert!(!frame.contains("gone"), "{frame}");
    }

    #[test]
    fn the_newest_toast_is_the_one_rendered() {
        let now = Instant::now();
        let mut toasts = Toasts::new();
        toasts.push_at("older".to_owned(), Tone::Neutral, TOAST_TTL, now);
        toasts.push_at("newer".to_owned(), Tone::Neutral, TOAST_TTL, now);

        let frame = drawn(&toasts, now);
        assert!(frame.contains("newer"), "{frame}");
        assert!(!frame.contains("older"), "{frame}");
    }

    /// Rendering nothing is a no-op rather than a panic: a screen is drawn on every
    /// frame and most of them have nothing to announce.
    #[test]
    fn an_empty_queue_draws_nothing_at_all() {
        let toasts = Toasts::new();
        assert_eq!(drawn(&toasts, Instant::now()).trim(), "");
    }

    /// **The cap drops the oldest**, which is what makes a rank counted from the
    /// newest stable: the entry the player is pointing at does not move because
    /// something fell off the far end.
    #[test]
    fn the_cap_drops_the_oldest_and_keeps_the_newest() {
        let now = Instant::now();
        let mut toasts = Toasts::new();
        for index in 0..HISTORY_CAP + 10 {
            toasts.push_at(format!("entry {index}"), Tone::Neutral, TOAST_TTL, now);
        }

        assert_eq!(toasts.items.len(), HISTORY_CAP);
        let log: Vec<Rc<str>> = toasts.log(now).map(|(_, text)| text).collect();
        assert_eq!(&*log[0], &*format!("entry {}", HISTORY_CAP + 9));
        assert_eq!(&*log[HISTORY_CAP - 1], &*format!("entry {}", 10));
    }

    /// The log reads newest first and each entry knows how long ago it was said —
    /// in whole seconds, which the screen turns into `2m`.
    #[test]
    fn the_log_reads_newest_first_and_carries_each_age() {
        let start = Instant::now();
        let mut toasts = Toasts::new();
        toasts.push_at("older".to_owned(), Tone::Neutral, TOAST_TTL, start);
        toasts.push_at(
            "newer".to_owned(),
            Tone::Neutral,
            TOAST_TTL,
            start + Duration::from_secs(90),
        );

        let log: Vec<(u64, Rc<str>)> = toasts.log(start + Duration::from_secs(120)).collect();
        assert_eq!(log.len(), 2);
        assert_eq!((&*log[0].1, log[0].0), ("newer", 30));
        assert_eq!((&*log[1].1, log[1].0), ("older", 120));
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
            let now = Instant::now();
            let mut toasts = Toasts::new();
            toasts.push_at("news".to_owned(), tone, TOAST_TTL, now);

            let mut terminal = match Terminal::new(TestBackend::new(20, 5)) {
                Ok(terminal) => terminal,
                Err(infallible) => match infallible {},
            };
            if let Err(infallible) = terminal.draw(|frame| {
                toasts.render(frame, frame.area(), now);
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
