//! The terminal event source.
//!
//! A background thread polls crossterm and forwards what it reads down an
//! [`mpsc`] channel, so the render loop in [`crate::session::Session::run`] can block on
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
use ratatui::crossterm::event::{self, Event as CrosstermEvent, KeyEvent};

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
    /// The heartbeat: *wake up and look at the clock*.
    ///
    /// **It is a sampling rate, not a cadence**, and that distinction is what the
    /// tick loop is built on. The two rates that matter — the 20 tps simulation and
    /// the ~30 fps redraw — live in [`crate::app::App`] as deadlines compared against
    /// `Instant::now()`, so neither counts heartbeats and neither drifts when one
    /// arrives late. What this event guarantees is only that the loop is *given* the
    /// chance to look, often enough that both deadlines are met to within its own
    /// period. It arrives even when the player touches nothing.
    Tick,
    /// A key event, **of any kind** — press, auto-repeat, or release.
    ///
    /// Nothing is filtered here any more, and that is a requirement rather than a
    /// simplification: `Space` held down is the game's central interaction, and a
    /// terminal speaking the kitty keyboard protocol reports its release as a
    /// [`Release`] this variant would otherwise drop. The filter moved
    /// to [`crate::keymap::resolve`], which is where a key already becomes a meaning
    /// — and it has to be there rather than here, because *which* kinds are
    /// meaningful is a per-binding answer: only the mine key cares about a release,
    /// and every other binding must keep firing exactly once per press.
    ///
    /// [`Release`]: ratatui::crossterm::event::KeyEventKind::Release
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

/// Where [`crate::session::Session::run`] gets its events from.
///
/// **A trait with exactly one production implementor**, which is usually a smell and
/// here is the point: [`EventHandler`] below cannot run without a terminal — its
/// thread calls `event::poll`, which fails outside a tty and takes the thread with
/// it — so a `run` that named the concrete type could not be executed by a test at
/// all. The loop is the one piece of this crate where drawing, decoding and quitting
/// meet, and it was the only piece nothing could reach.
///
/// The seam is deliberately this narrow. It does not abstract the terminal, the
/// channel or the thread; it abstracts *"hand me the next event"*, which is the whole
/// of what the loop asks. A test implements it over a list and drives the real loop.
///
/// `&self` and not `&mut self`, matching [`EventHandler::next`]: receiving from an
/// [`mpsc::Receiver`] needs no exclusive borrow, and requiring one here would stop
/// the loop from lending `&self` to `keymap::resolve` in the same iteration.
pub trait Events {
    /// Blocks until the next event is available.
    fn next(&self) -> Result<Event>;
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
    /// The thread `expect`s on **poll and read** because it runs off the main thread
    /// and cannot return a [`Result`] to anyone: a terminal that can no longer be
    /// polled is unrecoverable, and the panic hook installed by [`ratatui::init()`]
    /// restores the terminal before the message prints.
    ///
    /// **A failed `send` is not in that class, and treating it as one was a bug.** The
    /// channel is unbounded, so `send` cannot fail for being full; the only way it
    /// fails is that the [`Receiver`](mpsc::Receiver) is gone — which happens exactly
    /// once, when [`Session::run`](crate::session::Session::run) returns and drops the
    /// handler it was given. That is the shutdown signal, not an error, and the thread
    /// answers it by returning. It used to `expect` instead, so **every** exit raced:
    /// between the loop breaking and the process ending, `main` still has to pop the
    /// keyboard flags and restore the screen — far longer than the 10 ms until the next
    /// tick — so the thread almost always got there first and panicked into the shell
    /// the player had just come back to.
    ///
    /// The run was never at risk: the write happens before `quit` is set
    /// (`Session::on_key`), so the panic followed a save that had already landed.
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
                        let delivered = match event::read().expect("unable to read event") {
                            // Every kind, releases included: see `Event::Key`. The
                            // one filter that would be safe here — dropping releases
                            // — is the one that would cost the mine key its stop.
                            CrosstermEvent::Key(e) => sender.send(Event::Key(e)),
                            CrosstermEvent::Resize(_, _) => sender.send(Event::Resize),
                            // Everything else — mouse, focus, paste — is ignored.
                            _ => Ok(()),
                        };
                        // The loop has ended and taken the receiver with it. Nothing
                        // left to report to, so the thread stops reporting.
                        if delivered.is_err() {
                            return;
                        }
                    }

                    if last_tick.elapsed() >= tick_rate {
                        if sender.send(Event::Tick).is_err() {
                            return;
                        }
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
    ///
    /// Kept as an inherent method as well as the [`Events`] implementation below, so
    /// that a caller holding a concrete `EventHandler` does not have to import the
    /// trait to ask it the one question it answers.
    ///
    /// **The `Err` arm cannot fire while `self` exists.** `recv` fails only once every
    /// sender is gone, and [`Self::sender`] is one — held for exactly that reason —
    /// so a disconnect is unreachable through this method. The [`Result`] matches
    /// `recv`'s own signature rather than describing a case a caller should plan for.
    /// The consequence is worth knowing: if the polling thread dies, this **blocks**
    /// rather than returning an error.
    ///
    /// [`Self::sender`]: EventHandler#structfield.sender
    pub fn next(&self) -> Result<Event> {
        Ok(self.receiver.recv()?)
    }
}

impl Events for EventHandler {
    fn next(&self) -> Result<Event> {
        Self::next(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `EventHandler` over a channel the test owns, with a thread that does
    /// nothing.
    ///
    /// [`EventHandler::new`] cannot be called here: its thread polls crossterm, which
    /// fails outside a tty and takes the thread down with it before a single tick is
    /// sent. What is testable is the *receiving* half — which is the half the render
    /// loop touches — so the struct is built field by field, with a thread that exits
    /// immediately standing in for the poller. That is legitimate rather than a
    /// workaround: `handler` is documented as detached and never joined, so a thread
    /// that has already finished is indistinguishable to every reader of this struct.
    fn handler_over(sender: &mpsc::Sender<Event>, receiver: mpsc::Receiver<Event>) -> EventHandler {
        EventHandler {
            sender: sender.clone(),
            receiver,
            handler: thread::spawn(|| {}),
        }
    }

    #[test]
    fn next_hands_back_what_the_thread_sent_in_order() {
        let (sender, receiver) = mpsc::channel();
        let events = handler_over(&sender, receiver);

        // The three kinds the loop dispatches on, queued before any are read: the
        // channel is a queue and not a slot, so a burst of input during a slow frame
        // has to arrive whole and in order rather than collapsing to the last one.
        for event in [Event::Tick, Event::Resize, Event::Tick] {
            // `assert!` rather than `unwrap`: this crate's lints refuse the panicking
            // helpers even in tests, and a named failure beats a `SendError` anyway.
            assert!(sender.send(event).is_ok(), "the receiver was dropped early");
        }
        assert!(matches!(events.next(), Ok(Event::Tick)));
        assert!(matches!(events.next(), Ok(Event::Resize)));
        assert!(matches!(events.next(), Ok(Event::Tick)));
    }

    #[test]
    fn a_producer_that_has_gone_does_not_close_the_channel() {
        // **The `sender` field's whole job, asserted.** It is held and never read, so
        // what it does is keep the channel open even after every *other* sender has
        // gone — which means `recv` cannot report a disconnect while the handler is
        // alive. Written with a producer that sends and then exits: the event it left
        // behind still arrives, and the channel is still open behind it.
        //
        // This is also why `next`'s error arm is unreachable in production, and why
        // the note on `next` says so. Asserting the *absence* of a disconnect is done
        // by reading the queued event rather than by calling `next` a second time —
        // the second call is precisely the one that would block forever, which is a
        // hang and not a failure, and a test that can hang is worse than no test.
        let (sender, receiver) = mpsc::channel();
        let events = handler_over(&sender, receiver);
        assert!(
            sender.send(Event::Tick).is_ok(),
            "the receiver was dropped early"
        );
        drop(sender);

        assert!(
            matches!(events.next(), Ok(Event::Tick)),
            "the queued event was lost with its producer"
        );
    }

    #[test]
    fn the_trait_and_the_inherent_method_answer_alike() {
        // The `Events` impl exists so `App::run` can be generic; it must stay a
        // forwarder. A second opinion here would mean the loop and a direct caller
        // could see different events from one handler.
        let (sender, receiver) = mpsc::channel();
        let events = handler_over(&sender, receiver);
        assert!(
            sender.send(Event::Resize).is_ok(),
            "the receiver was dropped early"
        );
        assert!(matches!(Events::next(&events), Ok(Event::Resize)));
    }
}
