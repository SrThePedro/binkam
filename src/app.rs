//! Application state and the fixed-rate main loop.

use std::time::{Duration, Instant};

use chrono::Local;
use ratatui::{
    DefaultTerminal, Frame, Terminal,
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{self, Event, KeyEventKind},
    style::{Color, Style},
    widgets::Block,
};

use crate::{cli::Cli, clock::ClockWidget};

/// Events arriving within this window after startup are ignored.
///
/// Without it, releasing the <kbd>Enter</kbd> key that launched the program
/// would immediately quit the screensaver.
const GRACE_PERIOD: Duration = Duration::from_millis(300);

/// Screensaver state.
#[derive(Debug)]
pub struct App {
    /// Cleared when the user asks to quit; ends the main loop.
    running: bool,
    /// How long a single frame should take at the configured FPS.
    frame_time: Duration,
    /// Time accumulated since the first frame, used for the grace period.
    uptime: Duration,
    /// The big clock drawn in the middle of the screen.
    clock: ClockWidget,
}

impl App {
    /// Build an app from parsed command line arguments.
    pub fn new(cli: &Cli) -> Self {
        Self {
            running: true,
            frame_time: cli.frame_time(),
            uptime: Duration::ZERO,
            clock: ClockWidget::new(),
        }
    }

    /// Run the main loop until the user presses a key.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        let mut last = Instant::now();

        while self.running {
            let now = Instant::now();
            let dt = now.duration_since(last);
            last = now;

            self.update(dt);
            terminal.draw(|frame| self.render(frame))?;

            // Spend whatever is left of the frame budget waiting for input.
            let timeout = self.frame_time.saturating_sub(now.elapsed());
            if event::poll(timeout)? {
                self.handle_event(&event::read()?);
                // Drain anything else that piled up so input stays responsive.
                while self.running && event::poll(Duration::ZERO)? {
                    self.handle_event(&event::read()?);
                }
            }
        }

        Ok(())
    }

    /// Advance simulation state by `dt`. No drawing happens here.
    pub fn update(&mut self, dt: Duration) {
        self.uptime = self.uptime.saturating_add(dt);
        // `naive_local` hands the widget wall-clock time in the user's zone.
        self.clock.update_time(Local::now().naive_local());
    }

    /// Draw the current state. Must not mutate anything.
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        // Animations will slot in between these two from phase 2 on.
        frame.render_widget(Block::new().style(Style::new().bg(Color::Black)), area);
        frame.render_widget(&self.clock, area);
    }

    /// React to a terminal event; any key press quits once the grace period is over.
    fn handle_event(&mut self, event: &Event) {
        if self.uptime < GRACE_PERIOD {
            return;
        }
        match event {
            // On Windows every key arrives twice (Press and Release); only Press counts.
            Event::Key(key) if key.kind == KeyEventKind::Press => self.running = false,
            // Resize, mouse, focus and paste events are not a reason to quit.
            _ => {}
        }
    }
}

/// Render one frame to an off-screen backend and print it as plain text.
///
/// This never touches the real terminal, which makes it safe to run from
/// scripts and from an agent session.
pub fn dump_frame(cli: &Cli) -> color_eyre::Result<()> {
    let (width, height) = cli.dump_size();
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    let mut app = App::new(cli);

    // Let time move a little so animations are not stuck on their first frame.
    app.update(Duration::from_millis(500));
    terminal.draw(|frame| app.render(frame))?;

    print!("{}", buffer_to_text(terminal.backend().buffer()));
    Ok(())
}

/// Flatten a buffer into newline separated rows of symbols.
fn buffer_to_text(buf: &Buffer) -> String {
    let area = buf.area;
    let mut out = String::with_capacity(usize::from(area.width + 1) * usize::from(area.height));
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventState, KeyModifiers};

    fn app() -> App {
        App::new(&Cli::parse_from(["kumsaati"]))
    }

    fn key(kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn key_press_stops_the_loop() {
        let mut app = app();
        app.update(GRACE_PERIOD);
        app.handle_event(&key(KeyEventKind::Press));
        assert!(!app.running);
    }

    #[test]
    fn key_release_is_ignored() {
        let mut app = app();
        app.update(GRACE_PERIOD);
        app.handle_event(&key(KeyEventKind::Release));
        assert!(app.running, "Windows sends Release after every Press");
    }

    #[test]
    fn events_during_grace_period_are_ignored() {
        let mut app = app();
        app.update(Duration::from_millis(100));
        app.handle_event(&key(KeyEventKind::Press));
        assert!(app.running);
    }

    #[test]
    fn resize_does_not_quit() {
        let mut app = app();
        app.update(GRACE_PERIOD);
        app.handle_event(&Event::Resize(10, 5));
        assert!(app.running);
    }

    #[test]
    fn dump_renders_requested_size() {
        let cli = Cli::parse_from(["kumsaati", "--dump-frame", "--size", "20x5"]);
        let (width, height) = cli.dump_size();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let app = App::new(&cli);
        terminal.draw(|frame| app.render(frame)).unwrap();

        let text = buffer_to_text(terminal.backend().buffer());
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(lines.iter().all(|l| l.chars().count() == 20));
    }

    #[test]
    fn tiny_terminal_does_not_panic() {
        let mut terminal = Terminal::new(TestBackend::new(1, 1)).unwrap();
        let app = app();
        terminal.draw(|frame| app.render(frame)).unwrap();
    }
}
