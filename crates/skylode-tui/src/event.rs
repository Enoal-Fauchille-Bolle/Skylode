//! The terminal event source.
//!
//! A background thread polls crossterm and forwards what it reads down an
//! [`mpsc`] channel, so the render loop in [`crate::app::App::run`] can block on
//! a single `next()` and receive both real input *and* a periodic [`Event::Tick`]
//! from one place. The thread exists because `event::read` blocks: draining it on
//! the render thread would stall drawing, and a fixed tick could not be
//! synthesised while we wait.
//!
//! [`mpsc`]: std::sync::mpsc

use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use color_eyre::Result;
use ratatui::crossterm::event::{self, Event as CrosstermEvent, KeyEvent, KeyEventKind};

/// A terminal event, the *raw* input vocabulary.
///
/// Deliberately kept apart from [`crate::action::Action`], the *semantic* one:
/// `Event` is what crossterm reports (a key was pressed), `Action` is what it
/// means here (advance the screen ring). The translation happens once, in
/// [`crate::keymap`], which is what makes the app logic testable without a
/// terminal. Mouse events are intentionally absent — Skylode is keyboard-only,
/// so mouse capture is never enabled and the variant would never fire.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// The fixed-rate heartbeat, used to expire toasts (and, later, to drive the
    /// game tick). It arrives even when the player touches nothing.
    Tick,
    /// A key was pressed. Releases and repeats are filtered out at the source.
    Key(KeyEvent),
    /// The terminal was resized.
    ///
    /// Carries no dimensions on purpose: `Frame::area` reports the size at draw
    /// time, so copying it into the event would create a second source of truth
    /// that can disagree with the first. The variant exists only to wake the loop
    /// so it redraws — including, later, into the "terminal too small" screen,
    /// which will measure the frame rather than trust this message.
    Resize,
}

/// Owns the event thread and the receiving end of its channel.
#[derive(Debug)]
pub struct EventHandler {
    /// The sending end, kept only so the channel stays open for the lifetime of
    /// the handler; all real sending happens from the clone inside the thread.
    #[expect(dead_code, reason = "held to keep the channel alive, never read")]
    sender: mpsc::Sender<Event>,
    /// The receiving end the render loop drains via [`Self::next`].
    receiver: mpsc::Receiver<Event>,
    /// A handle to the polling thread. Dropping it detaches the thread, which is
    /// fine: it runs until the process exits.
    #[expect(dead_code, reason = "detached on drop; never joined")]
    handler: thread::JoinHandle<()>,
}

impl EventHandler {
    /// Spawns the polling thread. `tick_rate` is the heartbeat period in
    /// milliseconds.
    ///
    /// The thread `expect`s on poll/read/send because it runs off the main
    /// thread and cannot return a [`Result`] to anyone: a terminal that can no
    /// longer be polled is unrecoverable, and the panic hook installed by
    /// [`ratatui::init()`] restores the terminal before the message prints.
    #[expect(
        clippy::expect_used,
        reason = "a background thread cannot propagate a Result; a dead terminal is unrecoverable"
    )]
    pub fn new(tick_rate: u64) -> Self {
        let tick_rate = Duration::from_millis(tick_rate);
        let (sender, receiver) = mpsc::channel();
        let handler = {
            let sender = sender.clone();
            thread::spawn(move || {
                let mut last_tick = Instant::now();
                loop {
                    let timeout = tick_rate
                        .checked_sub(last_tick.elapsed())
                        .unwrap_or(tick_rate);

                    if event::poll(timeout).expect("unable to poll for event") {
                        match event::read().expect("unable to read event") {
                            CrosstermEvent::Key(e) if e.kind == KeyEventKind::Press => {
                                sender.send(Event::Key(e))
                            }
                            CrosstermEvent::Resize(_, _) => sender.send(Event::Resize),
                            // Everything else — key releases, mouse, focus, paste — is ignored.
                            _ => Ok(()),
                        }
                        .expect("failed to send terminal event");
                    }

                    if last_tick.elapsed() >= tick_rate {
                        sender.send(Event::Tick).expect("failed to send tick event");
                        last_tick = Instant::now();
                    }
                }
            })
        };
        Self {
            sender,
            receiver,
            handler,
        }
    }

    /// Blocks until the next event is available and returns it.
    ///
    /// Blocking is what we want: with nothing to do, the render loop should sleep
    /// rather than spin, and the heartbeat guarantees it wakes at least every
    /// `tick_rate`.
    pub fn next(&self) -> Result<Event> {
        Ok(self.receiver.recv()?)
    }
}
