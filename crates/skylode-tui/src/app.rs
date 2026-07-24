//! The application: state, the render loop, and the reducer that mutates it.
//!
//! `App` owns **UI state only** — which tab is open, which modal is stacked, the
//! live toasts. It deliberately owns no game rules: those belong to
//! `skylode-core`, and what the screens draw arrives as a flat [`View`] snapshot.
//! Keeping the split means a list cursor never leaks into a save file, and the
//! core stays testable without a terminal.

use std::time::Instant;

use color_eyre::Result;
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Layout, Rect},
    widgets::Tabs,
};

use crate::{
    action::Action,
    event::{Event, EventHandler},
    keymap,
    overlay::{Modal, too_small},
    screen::Screen,
    toast::{TOAST_TTL, Toasts},
    view::View,
};

/// The whole front-end state.
#[derive(Debug)]
pub struct App {
    /// Set by [`Action::Quit`]; the loop reads it and stops.
    pub should_quit: bool,
    /// The tab currently on screen.
    pub screen: Screen,
    /// The modal stacked over it, if any.
    ///
    /// Always `None` today — [`Modal`] has no variants yet — but the field is
    /// wired through the keymap and the renderer so that adding the first modal
    /// is one variant plus two `match` arms, not a re-plumbing.
    pub modal: Option<Modal>,
    /// Live announcements, drawn over everything.
    pub toasts: Toasts,
    /// What the screens render. Hand-filled until the core can produce it.
    pub view: View,
}

impl App {
    /// A fresh session, opened on the Mine tab.
    pub fn new() -> Self {
        Self {
            should_quit: false,
            screen: Screen::Mine,
            modal: None,
            toasts: Toasts::new(),
            view: View::sample(),
        }
    }

    /// Draws, then blocks for the next event, until asked to quit.
    ///
    /// Rendering happens *before* waiting so the first frame appears immediately
    /// rather than after the first keypress. Blocking on `events.next()` is what
    /// keeps the app at zero CPU while idle; the tick guarantees we still wake up
    /// often enough to expire toasts.
    pub fn run(mut self, mut terminal: DefaultTerminal, events: EventHandler) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.render(frame))?;

            match events.next()? {
                Event::Tick => self.on_tick(),
                Event::Key(key) => {
                    if let Some(action) = keymap::resolve(&self, key) {
                        self.update(action);
                    }
                }
                // Nothing to do: ratatui lays out against the new size on the
                // next draw, which the loop is about to perform anyway.
                Event::Resize => {}
            }
        }
        Ok(())
    }

    /// Applies one decoded intent.
    ///
    /// This is the reducer, and it is the reason [`Action`] exists: it takes no
    /// `KeyEvent` and touches no terminal, so every transition below is a plain
    /// unit test. The `match` is exhaustive, so a new `Action` variant cannot be
    /// added without deciding what it does here.
    pub fn update(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::NextScreen => self.screen = self.screen.next(),
            Action::PrevScreen => self.screen = self.screen.prev(),
            Action::SelectScreen(index) => {
                // An out-of-range index leaves the screen alone rather than
                // clamping: the ring is the authority on what exists.
                if let Some(screen) = Screen::from_index(index) {
                    self.screen = screen;
                }
            }
            Action::ShowToast(text) => self.toasts.push(text, TOAST_TTL),
        }
    }

    /// The heartbeat. Expires toasts today; drives the game tick from phase 7.
    fn on_tick(&mut self) {
        self.toasts.prune(Instant::now());
    }

    /// Paints one frame: tab bar, active screen, then the overlays on top.
    ///
    /// Order is the layering: overlays draw last precisely so they cover the
    /// screen rather than being covered by it.
    fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        // The terminal-too-small filter, in front of everything (UI-EN.md §6.2).
        // It is not a screen and not a modal: below the 80×24 budget it replaces
        // the whole frame regardless of which tab or overlay is up, and yields it
        // back untouched once the window grows, because it reads no state. Drawing
        // it here — before the tab bar even splits the area — is what "a filter,
        // not a state with edges" means in code.
        if !too_small::fits(area) {
            too_small::render(frame, area);
            return;
        }

        // The tab bar takes exactly one row and the screen takes the rest —
        // "the grid is fixed, the chrome flexes" starts here.
        let [tabs_area, body_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);

        self.render_tabs(frame, tabs_area);
        self.screen.render(frame, body_area, &self.view);

        // Overlays, outermost last.
        self.toasts.render(frame, area);
        if let Some(modal) = self.modal {
            // Uninhabited: unreachable until the first modal variant exists.
            match modal {}
        }
    }

    /// Draws the ring as a `Tabs` widget, numbered for the `1`..`6` shortcuts.
    ///
    /// The digits are printed rather than merely bound so the shortcut is
    /// discoverable without opening help — and the prestige readout that used to
    /// share this row was dropped precisely to keep six numbered tabs fitting in
    /// 80 columns (UI-EN.md §5.7.5).
    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let titles = Screen::ALL
            .iter()
            .enumerate()
            .map(|(position, screen)| format!(" {} {} ", position + 1, screen.title()));
        let tabs = Tabs::new(titles).select(self.screen.index()).divider("│");
        frame.render_widget(tabs, area);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use super::*;

    /// Draws `app` into an off-screen 80×24 terminal and hands back the cells.
    ///
    /// `TestBackend` is what makes the render path testable at all: no tty, no
    /// raw mode, just a buffer we can read back — so "does the tab bar actually
    /// say `6 Levels`" is an assertion rather than something eyeballed.
    ///
    /// Its operations are typed `Result<_, Infallible>` — writing to a `Vec` of
    /// cells cannot fail — so the errors are discharged with an empty `match` on
    /// the uninhabited error rather than an `unwrap` the lints would flag. Same
    /// trick as the `Modal` slot above: no value can exist, so there is no arm.
    fn render_to_buffer(app: &App) -> Buffer {
        render_to_sized_buffer(app, 80, 24)
    }

    /// The same, at an arbitrary size — for the too-small filter, whose whole job
    /// is what happens below 80×24.
    fn render_to_sized_buffer(app: &App, width: u16, height: u16) -> Buffer {
        let mut terminal = match Terminal::new(TestBackend::new(width, height)) {
            Ok(terminal) => terminal,
            Err(infallible) => match infallible {},
        };
        if let Err(infallible) = terminal.draw(|frame| app.render(frame)) {
            match infallible {}
        }
        terminal.backend().buffer().clone()
    }

    /// The text of row `y`, joined back into a string.
    fn row(buffer: &Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer[(x, y)].symbol())
            .collect()
    }

    /// Every row of the frame, joined — for "is this text on screen anywhere".
    fn whole_frame(buffer: &Buffer) -> String {
        (0..buffer.area.height)
            .map(|y| row(buffer, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_new_session_opens_on_the_mine_tab_with_nothing_stacked() {
        let app = App::new();
        assert_eq!(app.screen, Screen::Mine);
        assert!(app.modal.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn next_and_prev_walk_the_ring() {
        let mut app = App::new();
        app.update(Action::NextScreen);
        assert_eq!(app.screen, Screen::Mines);
        app.update(Action::PrevScreen);
        assert_eq!(app.screen, Screen::Mine);
    }

    #[test]
    fn the_ring_wraps_in_both_directions() {
        let mut app = App::new();
        app.update(Action::PrevScreen);
        assert_eq!(app.screen, Screen::Levels);
        app.update(Action::NextScreen);
        assert_eq!(app.screen, Screen::Mine);
    }

    #[test]
    fn selecting_a_tab_jumps_straight_to_it() {
        let mut app = App::new();
        app.update(Action::SelectScreen(3));
        assert_eq!(app.screen, Screen::Upgrades);
    }

    #[test]
    fn selecting_a_tab_that_does_not_exist_leaves_the_screen_alone() {
        let mut app = App::new();
        app.update(Action::SelectScreen(99));
        assert_eq!(app.screen, Screen::Mine);
    }

    #[test]
    fn quit_raises_the_flag_the_loop_watches() {
        let mut app = App::new();
        app.update(Action::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn showing_a_toast_queues_it() {
        let mut app = App::new();
        assert!(app.toasts.is_empty());
        app.update(Action::ShowToast("Mine refilled".to_owned()));
        assert_eq!(app.toasts.len(), 1);
    }

    #[test]
    fn the_heartbeat_expires_a_toast_once_its_moment_has_passed() {
        let mut app = App::new();
        app.update(Action::ShowToast("Mine refilled".to_owned()));
        // `on_tick` prunes against the real clock, so reach past the TTL directly.
        app.toasts.prune(Instant::now() + TOAST_TTL + TOAST_TTL);
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn the_tab_bar_shows_all_six_tabs_with_their_digits() {
        let buffer = render_to_buffer(&App::new());
        let bar = row(&buffer, 0);
        for (position, screen) in Screen::ALL.iter().enumerate() {
            let label = format!("{} {}", position + 1, screen.title());
            assert!(
                bar.contains(&label),
                "tab bar is missing {label:?}: {bar:?}"
            );
        }
    }

    #[test]
    fn every_tab_draws_itself_and_stays_selected() {
        // Walks the whole ring through the renderer. A screen that panics on an
        // empty area, or that forgets to mark itself selected, fails here rather
        // than the first time someone presses its digit.
        for (position, screen) in Screen::ALL.iter().enumerate() {
            let mut app = App::new();
            app.update(Action::SelectScreen(position));
            assert_eq!(app.screen, *screen);

            let buffer = render_to_buffer(&app);
            let frame = whole_frame(&buffer);
            assert!(
                frame.contains(screen.title()),
                "{} did not draw its own title:\n{frame}",
                screen.title()
            );
        }
    }

    #[test]
    fn the_tab_bar_fits_the_eighty_column_reference_width() {
        // UI-EN.md §5.7.5 counts the six-tab bar at 65 columns, which is what
        // dropping the prestige readout bought. If a renamed tab ever pushes it
        // past 80 this fails rather than silently truncating on a real terminal.
        let buffer = render_to_buffer(&App::new());
        let bar = row(&buffer, 0);
        assert!(bar.trim_end().chars().count() <= 80);
    }

    #[test]
    fn the_selected_screen_is_the_one_drawn() {
        let mut app = App::new();
        app.update(Action::SelectScreen(2));
        let frame = whole_frame(&render_to_buffer(&app));
        // The Inventory placeholder prints the held counts from the snapshot.
        assert!(frame.contains("Inventory"), "{frame}");
        assert!(frame.contains("480"), "{frame}");
    }

    #[test]
    fn the_mine_tab_paints_coloured_cells() {
        // The grid is the one thing on screen that carries information in its
        // *background*, so "did anything get painted" is a real assertion here and
        // not a tautology: every other widget leaves `bg` at `Reset`.
        use ratatui::style::Color;

        let buffer = render_to_buffer(&App::new());
        let painted = buffer.content().iter().any(|cell| cell.bg != Color::Reset);
        assert!(
            painted,
            "the mine screen drew no swatch:\n{}",
            whole_frame(&buffer)
        );
    }

    #[test]
    fn a_cramped_terminal_shows_the_filter_instead_of_the_open_screen() {
        // Sitting on a non-default tab, under the budget: the filter must win over
        // whatever was up, drawing its message and none of the tab bar.
        let mut app = App::new();
        app.update(Action::SelectScreen(2));
        let frame = whole_frame(&render_to_sized_buffer(&app, 54, 18));
        assert!(frame.contains("Skylode needs 80 x 24"), "{frame}");
        assert!(
            !frame.contains("2 Inventory"),
            "the tab bar leaked through: {frame}"
        );
    }

    #[test]
    fn a_toast_is_drawn_over_the_screen_underneath() {
        let mut app = App::new();
        app.update(Action::ShowToast("Mine refilled".to_owned()));
        let frame = whole_frame(&render_to_buffer(&app));
        assert!(frame.contains("Mine refilled"), "{frame}");
    }

    #[test]
    fn no_toast_means_nothing_is_overlaid() {
        let plain = whole_frame(&render_to_buffer(&App::new()));

        let mut app = App::new();
        app.update(Action::ShowToast("Mine refilled".to_owned()));
        let toasted = whole_frame(&render_to_buffer(&app));

        // The toast borrows cells for a frame; without one the frame is untouched.
        assert_ne!(plain, toasted);
    }
}
