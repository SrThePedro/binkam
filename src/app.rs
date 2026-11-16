//! Application state and the fixed-rate main loop.

use std::{
    fmt::Write as _,
    time::{Duration, Instant},
};

use chrono::Local;
use color_eyre::eyre::eyre;
use ratatui::{
    DefaultTerminal, Frame, Terminal,
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{self, Event, KeyEventKind},
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Widget as _},
};

use crate::{anim::Animation, cli::Cli, clock::ClockWidget, render};

/// Events arriving within this window after startup are ignored.
///
/// Without it, releasing the <kbd>Enter</kbd> key that launched the program
/// would immediately quit the screensaver.
const GRACE_PERIOD: Duration = Duration::from_millis(300);

/// Extra columns and rows darkened around the clock.
const BACKDROP_PAD_X: u16 = 3;
const BACKDROP_PAD_Y: u16 = 1;

/// Fraction of brightness left in the animation behind the clock.
const BACKDROP_KEEP: f32 = 0.28;

/// Weight of the newest sample in the smoothed frames-per-second estimate.
const FPS_SMOOTHING: f32 = 0.1;

/// Frames simulated before `--dump-frame` draws, so the output shows a
/// settled animation rather than its first frame.
const DUMP_WARMUP_FRAMES: u32 = 90;

/// Screensaver state.
pub struct App {
    /// Cleared when the user asks to quit; ends the main loop.
    running: bool,
    /// How long a single frame should take at the configured FPS.
    frame_time: Duration,
    /// Time accumulated since the first frame, used for the grace period.
    uptime: Duration,
    /// The big clock drawn in the middle of the screen.
    clock: ClockWidget,
    /// The background effect, chosen by name at startup.
    ///
    /// `Box<dyn Animation>` because every effect is a different concrete type.
    animation: Box<dyn Animation>,
    /// Last area the animation was told about, so resizes are only forwarded
    /// when the terminal really changed.
    area: Rect,
    /// Whether to darken the animation behind the clock.
    backdrop: bool,
    /// Whether to draw the frames-per-second counter.
    show_fps: bool,
    /// Smoothed frames per second.
    fps: f32,
    /// Pre-rendered counter text, so drawing never formats or allocates.
    fps_text: String,
}

impl App {
    /// Build an app from parsed command line arguments.
    pub fn new(cli: &Cli) -> color_eyre::Result<Self> {
        let animation = crate::anim::by_name(&cli.anim, cli.resolved_seed())
            .ok_or_else(|| eyre!("unknown animation `{}`; try --list", cli.anim))?;

        Ok(Self {
            running: true,
            frame_time: cli.frame_time(),
            uptime: Duration::ZERO,
            clock: ClockWidget::new(),
            animation,
            area: Rect::ZERO,
            backdrop: !cli.no_backdrop,
            show_fps: cli.show_fps,
            fps: 0.0,
            fps_text: String::with_capacity(8),
        })
    }

    /// Run the main loop until the user presses a key.
    pub fn run(mut self, mut terminal: DefaultTerminal) -> color_eyre::Result<()> {
        let mut last = Instant::now();

        while self.running {
            let now = Instant::now();
            let dt = now.duration_since(last);
            last = now;

            let size = terminal.size()?;
            self.resize(Rect::new(0, 0, size.width, size.height));
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

    /// Tell the animation about a new terminal size, if it actually changed.
    pub fn resize(&mut self, area: Rect) {
        if self.area != area {
            self.area = area;
            self.animation.resize(area);
        }
    }

    /// Advance simulation state by `dt`. No drawing happens here.
    pub fn update(&mut self, dt: Duration) {
        self.uptime = self.uptime.saturating_add(dt);
        // `naive_local` hands the widget wall-clock time in the user's zone.
        self.clock.update_time(Local::now().naive_local());
        self.animation.update(dt);
        self.update_fps(dt);
    }

    /// Keep the smoothed frame rate and its pre-rendered text up to date.
    fn update_fps(&mut self, dt: Duration) {
        if !self.show_fps || dt.is_zero() {
            return;
        }
        let sample = 1.0 / dt.as_secs_f32();
        self.fps = if self.fps == 0.0 {
            sample
        } else {
            self.fps + (sample - self.fps) * FPS_SMOOTHING
        };

        // The animation name rides along; it is the other thing worth seeing
        // while debugging a frame rate problem.
        self.fps_text.clear();
        let _ = write!(
            self.fps_text,
            "{} · {:.0} fps",
            self.animation.name(),
            self.fps
        );
    }

    /// Draw the current state. Must not mutate anything.
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::new().style(Style::new().bg(Color::Black)), area);

        let buf = frame.buffer_mut();
        self.animation.render(area, buf);

        // Dim the animation just behind the clock so the digits stay legible.
        if self.backdrop
            && let Some(clock_area) = self.clock.occupied_area(area)
        {
            render::fade_to_black(buf, backdrop_rect(clock_area, area), BACKDROP_KEEP);
        }

        self.clock.render(area, buf);
        self.render_fps(area, buf);
    }

    /// Draw the counter in the top right corner, if it is enabled.
    fn render_fps(&self, area: Rect, buf: &mut Buffer) {
        if !self.show_fps {
            return;
        }
        let width = render::text_cols(&self.fps_text);
        if width == 0 || area.width < width || area.height == 0 {
            return;
        }
        buf.set_string(
            area.right() - width,
            area.y,
            &self.fps_text,
            Style::new().fg(Color::DarkGray),
        );
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

/// The rectangle to darken behind the clock, padded and clipped to `area`.
fn backdrop_rect(clock: Rect, area: Rect) -> Rect {
    Rect::new(
        clock.x.saturating_sub(BACKDROP_PAD_X),
        clock.y.saturating_sub(BACKDROP_PAD_Y),
        clock.width.saturating_add(BACKDROP_PAD_X * 2),
        clock.height.saturating_add(BACKDROP_PAD_Y * 2),
    )
    .intersection(area)
}

/// Render one frame to an off-screen backend and print it as plain text.
///
/// This never touches the real terminal, which makes it safe to run from
/// scripts and from an agent session.
pub fn dump_frame(cli: &Cli) -> color_eyre::Result<()> {
    let (width, height) = cli.dump_size();
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    let mut app = App::new(cli)?;

    app.resize(Rect::new(0, 0, width, height));
    // Simulate a few seconds so the picture is representative: effects that
    // fall in from off screen look empty on their first frame.
    for _ in 0..DUMP_WARMUP_FRAMES {
        app.update(cli.frame_time());
    }
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

    fn app_from(args: &[&str]) -> App {
        let mut full = vec!["kumsaati"];
        full.extend_from_slice(args);
        App::new(&Cli::parse_from(full)).expect("the default animation must exist")
    }

    fn app() -> App {
        app_from(&[])
    }

    fn key(kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code: KeyCode::Char('q'),
            modifiers: KeyModifiers::NONE,
            kind,
            state: KeyEventState::NONE,
        })
    }

    /// Draw one frame of `app` at the given size and return the raw buffer.
    fn frame_buffer(app: &mut App, width: u16, height: u16) -> Buffer {
        let area = Rect::new(0, 0, width, height);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        app.resize(area);
        app.update(Duration::from_millis(33));
        terminal.draw(|frame| app.render(frame)).unwrap();
        terminal.backend().buffer().clone()
    }

    /// Same, but only the characters, with styles thrown away.
    fn frame(app: &mut App, width: u16, height: u16) -> Vec<String> {
        buffer_to_text(&frame_buffer(app, width, height))
            .lines()
            .map(str::to_owned)
            .collect()
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
    fn resize_is_only_forwarded_when_the_size_changes() {
        let mut app = app();
        let area = Rect::new(0, 0, 40, 12);
        app.resize(area);
        assert_eq!(app.area, area);
        app.resize(area); // same size again, must be a no-op
        assert_eq!(app.area, area);
    }

    #[test]
    fn unknown_animation_is_an_error() {
        let mut cli = Cli::parse_from(["kumsaati"]);
        cli.anim = "nope".to_owned();
        assert!(App::new(&cli).is_err());
    }

    #[test]
    fn draws_the_animation_behind_the_clock() {
        let mut app = app_from(&["--seed", "11"]);
        let rows = frame(&mut app, 80, 24);
        let text = rows.join("\n");

        assert!(text.contains('█'), "no clock digits");
        assert!(
            text.contains('.') || text.contains('+') || text.contains('*'),
            "no stars drawn"
        );
    }

    #[test]
    fn the_backdrop_dims_stars_near_the_clock() {
        let (width, height) = (80u16, 24u16);
        let area = Rect::new(0, 0, width, height);

        let mut bare = app_from(&["--seed", "11", "--no-backdrop"]);
        let lit = frame_buffer(&mut bare, width, height);
        let mut shaded = app_from(&["--seed", "11"]);
        let dimmed = frame_buffer(&mut shaded, width, height);

        let box_area = backdrop_rect(shaded.clock.occupied_area(area).unwrap(), area);
        let mut dimmed_cells = 0;

        for y in 0..height {
            for x in 0..width {
                // Only the animation uses Rgb; the clock uses named colours.
                let (Color::Rgb(before, _, _), Color::Rgb(after, _, _)) =
                    (lit[(x, y)].fg, dimmed[(x, y)].fg)
                else {
                    continue;
                };
                if box_area.contains((x, y).into()) {
                    assert!(after <= before, "star at {x},{y} got brighter");
                    dimmed_cells += usize::from(after < before);
                } else {
                    assert_eq!(before, after, "star at {x},{y} changed outside the box");
                }
            }
        }

        assert!(dimmed_cells > 0, "no star behind the clock was dimmed");
    }

    #[test]
    fn the_backdrop_never_changes_which_characters_are_drawn() {
        let mut bare = app_from(&["--seed", "11", "--no-backdrop"]);
        let mut shaded = app_from(&["--seed", "11"]);
        assert_eq!(frame(&mut bare, 80, 24), frame(&mut shaded, 80, 24));
    }

    #[test]
    fn fps_counter_is_off_by_default_and_on_with_the_flag() {
        let mut off = app_from(&["--seed", "3"]);
        assert!(!frame(&mut off, 80, 24).join("\n").contains("fps"));

        let mut on = app_from(&["--seed", "3", "--show-fps"]);
        assert!(frame(&mut on, 80, 24)[0].trim_end().ends_with("fps"));
    }

    #[test]
    fn dump_renders_requested_size() {
        let cli = Cli::parse_from(["kumsaati", "--dump-frame", "--size", "20x5"]);
        let (width, height) = cli.dump_size();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let app = App::new(&cli).unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();

        let text = buffer_to_text(terminal.backend().buffer());
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(lines.iter().all(|l| l.chars().count() == 20));
    }

    #[test]
    fn tiny_terminals_do_not_panic() {
        let mut app = app_from(&["--seed", "2", "--show-fps"]);
        for (w, h) in [(1, 1), (3, 2), (5, 1), (20, 5), (47, 5)] {
            let _ = frame(&mut app, w, h);
        }
    }
}
