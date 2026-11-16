//! Application state and the fixed-rate main loop.

use std::{
    fmt::Write as _,
    time::{Duration, Instant},
};

use chrono::Local;
use color_eyre::eyre::eyre;
use rand::{RngExt, SeedableRng, rngs::StdRng};
use ratatui::{
    DefaultTerminal, Frame, Terminal,
    backend::TestBackend,
    buffer::Buffer,
    crossterm::event::{self, Event, KeyCode, KeyEventKind},
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Widget as _},
};

use crate::{
    anim::Animation,
    cli::Cli,
    clock::{
        ClockWidget,
        motion::{ClockMotion, ClockPosition},
    },
    render,
    theme::Theme,
};

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

/// Fraction left behind the one-line readouts, which are small enough to be
/// swallowed by a busy animation without this.
const OVERLAY_KEEP: f32 = 0.12;

/// Weight of the newest sample in the smoothed frames-per-second estimate.
const FPS_SMOOTHING: f32 = 0.1;

/// Frames simulated before `--dump-frame` draws, so the output shows a
/// settled animation rather than its first frame.
const DUMP_WARMUP_FRAMES: u32 = 90;

/// How long a change of animation takes, from lit through black and back.
const TRANSITION: Duration = Duration::from_millis(900);

/// Key reminder shown along the bottom in preview mode.
const PREVIEW_HINT: &str = "n/p animation · t theme · c position · q quit";

/// A change from one animation to the next, fading out and back in.
struct Transition {
    /// Runs from 0.0 to 1.0; the screen is fully black halfway through.
    progress: f32,
    /// The animation to put on screen at the halfway point, taken once used.
    next: Option<Box<dyn Animation>>,
}

/// Screensaver state.
pub struct App {
    /// Cleared when the user asks to quit; ends the main loop.
    running: bool,
    /// How long a single frame should take at the configured FPS.
    frame_time: Duration,
    /// Time accumulated since the first frame, used for the grace period.
    uptime: Duration,
    /// The big clock.
    clock: ClockWidget,
    /// Where the clock sits, and how it moves.
    motion: ClockMotion,
    /// The background effect, chosen by name at startup.
    ///
    /// `Box<dyn Animation>` because every effect is a different concrete type.
    animation: Box<dyn Animation>,
    /// Last area the animation was told about, so resizes are only forwarded
    /// when the terminal really changed.
    area: Rect,
    /// Colours for the clock and for whatever the animation draws.
    theme: Theme,
    /// Whether to darken the animation behind the clock.
    backdrop: bool,
    /// Randomness for picking the next animation and seeding it.
    rng: StdRng,
    /// Index of the running animation in `anim::all_names`.
    anim_index: usize,
    /// How long to wait between animation changes, or `None` for never.
    cycle_interval: Option<Duration>,
    /// Time since the last change of animation.
    since_switch: Duration,
    /// The fade in progress, if any.
    transition: Option<Transition>,
    /// Index of the running theme in `Theme::all_names`.
    theme_index: usize,
    /// Whether keys browse the combinations instead of quitting.
    preview: bool,
    /// Whether to draw the status and frames-per-second readout.
    show_fps: bool,
    /// Smoothed frames per second.
    fps: f32,
    /// Pre-rendered counter text, so drawing never formats or allocates.
    fps_text: String,
}

impl App {
    /// Build an app from parsed command line arguments.
    pub fn new(cli: &Cli) -> color_eyre::Result<Self> {
        let mut rng = StdRng::seed_from_u64(cli.resolved_seed());
        let names = crate::anim::all_names();

        let anim_index = match &cli.anim {
            Some(name) => names
                .iter()
                .position(|known| known == name)
                .ok_or_else(|| eyre!("unknown animation `{name}`; try --list"))?,
            // Nothing pinned: start somewhere random and keep changing.
            None => rng.random_range(0..names.len()),
        };
        let animation = crate::anim::by_name(names[anim_index], rng.random::<u64>())
            .ok_or_else(|| eyre!("unknown animation `{}`; try --list", names[anim_index]))?;

        let theme_names = Theme::all_names();
        let theme_index = theme_names
            .iter()
            .position(|known| *known == cli.theme)
            .ok_or_else(|| eyre!("unknown theme `{}`; try --list-themes", cli.theme))?;
        let theme = Theme::by_name(theme_names[theme_index])
            .ok_or_else(|| eyre!("unknown theme `{}`; try --list-themes", cli.theme))?;

        let position = ClockPosition::by_name(&cli.clock_position)
            .ok_or_else(|| eyre!("unknown clock position `{}`", cli.clock_position))?;

        let mut clock = ClockWidget::new();
        clock.digit_color = theme.clock_color();
        clock.date_color = theme.date_color();
        // Give the clock a time straight away, so its size can be measured on
        // the very first frame instead of coming out empty.
        clock.update_time(Local::now().naive_local());

        Ok(Self {
            running: true,
            frame_time: cli.frame_time(),
            uptime: Duration::ZERO,
            clock,
            motion: ClockMotion::new(position),
            animation,
            area: Rect::ZERO,
            rng,
            anim_index,
            cycle_interval: cli.cycle_interval(),
            since_switch: Duration::ZERO,
            transition: None,
            theme,
            theme_index,
            preview: cli.preview,
            backdrop: !cli.no_backdrop,
            // Preview mode needs the readout to say what is on screen.
            show_fps: cli.show_fps || cli.preview,
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
        if self.area == area {
            return;
        }
        self.area = area;
        self.animation.resize(area);
        // The animation waiting to fade in needs the new size too, or it
        // would arrive laid out for the old one.
        if let Some(next) = self.transition.as_mut().and_then(|t| t.next.as_mut()) {
            next.resize(area);
        }
    }

    /// Start fading over to the animation at `index`.
    fn start_switch(&mut self, index: usize) {
        let names = crate::anim::all_names();
        let index = index % names.len();
        let Some(mut next) = crate::anim::by_name(names[index], self.rng.random::<u64>()) else {
            return;
        };

        next.resize(self.area);
        self.anim_index = index;
        self.since_switch = Duration::ZERO;
        self.transition = Some(Transition {
            progress: 0.0,
            next: Some(next),
        });
    }

    /// A random animation that is not the one already running.
    fn other_animation(&mut self) -> usize {
        let count = crate::anim::all_names().len();
        if count < 2 {
            return self.anim_index;
        }
        // Draw from the other animations by skipping over the current one.
        let mut index = self.rng.random_range(0..count - 1);
        if index >= self.anim_index {
            index += 1;
        }
        index
    }

    /// Move a running fade along, swapping animations at the halfway point.
    fn advance_transition(&mut self, dt: Duration) {
        let Some(transition) = self.transition.as_mut() else {
            return;
        };

        transition.progress += dt.as_secs_f32() / TRANSITION.as_secs_f32();
        // The old animation is fully dark at the midpoint: the moment to
        // swap, so neither one is ever seen appearing.
        let arrived = (transition.progress >= 0.5)
            .then(|| transition.next.take())
            .flatten();
        let finished = transition.progress >= 1.0;

        if let Some(animation) = arrived {
            self.animation = animation;
        }
        if finished {
            self.transition = None;
        }
    }

    /// How much of the animation's brightness survives the current fade.
    fn transition_keep(&self) -> f32 {
        self.transition.as_ref().map_or(1.0, |transition| {
            // 1.0 at both ends, 0.0 in the middle.
            (transition.progress * 2.0 - 1.0).abs().clamp(0.0, 1.0)
        })
    }

    /// Advance simulation state by `dt`. No drawing happens here.
    pub fn update(&mut self, dt: Duration) {
        self.uptime = self.uptime.saturating_add(dt);
        // `naive_local` hands the widget wall-clock time in the user's zone.
        self.clock.update_time(Local::now().naive_local());

        // Measure the clock every frame: its size changes with the terminal
        // and, at midnight, with the length of the date line.
        self.motion
            .set_bounds(self.area, self.clock.measure(self.area));
        self.motion.update(dt);
        if self.motion.position() == ClockPosition::Bounce {
            // A DVD logo picks a new colour every time it hits an edge.
            self.clock.digit_color = self.theme.accent(self.motion.bounces());
        }

        if let Some(interval) = self.cycle_interval {
            self.since_switch = self.since_switch.saturating_add(dt);
            if self.since_switch >= interval && self.transition.is_none() {
                let index = self.other_animation();
                self.start_switch(index);
            }
        }
        self.advance_transition(dt);

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

        // The animation, theme and clock position ride along; they are the
        // other things worth seeing while debugging a frame rate problem.
        self.fps_text.clear();
        let _ = write!(
            self.fps_text,
            "{} · {} · {} · {:.0} fps",
            self.animation.name(),
            self.theme.name(),
            self.motion.position().name(),
            self.fps
        );
    }

    /// Draw the current state. Must not mutate anything.
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::new().style(Style::new().bg(Color::Black)), area);

        let buf = frame.buffer_mut();
        self.animation.render(area, buf);

        // Re-colour the animation before anything is dimmed: the theme maps
        // colours by brightness, so running it after a fade would undo it.
        self.theme.apply(buf, area);

        // Fade the animation layer, not the clock: a change of animation dips
        // through black while the time stays readable.
        let keep = self.transition_keep();
        if keep < 1.0 {
            render::fade_to_black(buf, area, keep);
        }

        let clock_area = self.motion.rect();

        // Dim the animation just behind the clock so the digits stay legible.
        if self.backdrop
            && let Some(occupied) = self.clock.occupied_area(clock_area)
        {
            render::fade_to_black(buf, backdrop_rect(occupied, area), BACKDROP_KEEP);
        }

        self.clock.render(clock_area, buf);
        self.render_fps(area, buf);
        self.render_hint(area, buf);
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
        let x = area.right() - width;
        render::fade_to_black(buf, Rect::new(x, area.y, width, 1), OVERLAY_KEEP);
        buf.set_string(x, area.y, &self.fps_text, Style::new().fg(Color::DarkGray));
    }

    /// React to a terminal event.
    ///
    /// Outside preview mode any key press quits, once the grace period is
    /// over. Resize, mouse, focus and paste events never are a reason to.
    fn handle_event(&mut self, event: &Event) {
        if self.uptime < GRACE_PERIOD {
            return;
        }
        // On Windows every key arrives twice (Press and Release); only Press
        // counts, or the program would quit the moment it started.
        let Event::Key(key) = event else { return };
        if key.kind != KeyEventKind::Press {
            return;
        }

        if !self.preview {
            self.running = false;
            return;
        }

        let count = crate::anim::all_names().len();
        match key.code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => self.running = false,
            KeyCode::Char('n' | 'N') | KeyCode::Right => self.start_switch(self.anim_index + 1),
            KeyCode::Char('p' | 'P') | KeyCode::Left => {
                self.start_switch(self.anim_index + count - 1);
            }
            KeyCode::Char('t' | 'T') => self.next_theme(),
            KeyCode::Char('c' | 'C') => self.next_clock_position(),
            _ => {}
        }
    }

    /// Move on to the next theme in the list.
    fn next_theme(&mut self) {
        let names = Theme::all_names();
        self.theme_index = (self.theme_index + 1) % names.len();
        if let Some(theme) = Theme::by_name(names[self.theme_index]) {
            self.theme = theme;
            self.clock.date_color = self.theme.date_color();
            // Bounce paints the digits itself, from the new theme's accents.
            if self.motion.position() != ClockPosition::Bounce {
                self.clock.digit_color = self.theme.clock_color();
            }
        }
    }

    /// Move the clock on to the next position mode.
    fn next_clock_position(&mut self) {
        let names = ClockPosition::all_names();
        let current = self.motion.position().name();
        let index = names.iter().position(|name| *name == current).unwrap_or(0);
        let Some(position) = ClockPosition::by_name(names[(index + 1) % names.len()]) else {
            return;
        };

        self.motion.set_position(position);
        if position != ClockPosition::Bounce {
            // Take back the digit colour that bounce was cycling.
            self.clock.digit_color = self.theme.clock_color();
        }
    }

    /// Draw the key reminder along the bottom, in preview mode only.
    fn render_hint(&self, area: Rect, buf: &mut Buffer) {
        if !self.preview || area.height == 0 {
            return;
        }
        let width = render::text_cols(PREVIEW_HINT);
        if area.width < width {
            return;
        }
        let x = area.x + (area.width - width) / 2;
        let y = area.bottom() - 1;
        render::fade_to_black(buf, Rect::new(x, y, width, 1), OVERLAY_KEEP);
        buf.set_string(x, y, PREVIEW_HINT, Style::new().fg(Color::DarkGray));
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
        cli.anim = Some("nope".to_owned());
        assert!(App::new(&cli).is_err());
    }

    #[test]
    fn a_pinned_animation_never_changes() {
        let mut app = app_from(&["--anim", "fire", "--seed", "1"]);
        assert_eq!(app.cycle_interval, None);
        for _ in 0..200 {
            app.update(Duration::from_secs(10));
        }
        assert_eq!(app.animation.name(), "fire");
        assert!(app.transition.is_none());
    }

    #[test]
    fn cycling_changes_the_animation_through_a_fade() {
        let mut app = app_from(&["--cycle-minutes", "1", "--seed", "1"]);
        app.resize(Rect::new(0, 0, 40, 12));
        let first = app.animation.name();
        let tick = Duration::from_millis(33);

        // Realistic frames rather than one huge step, which would run the
        // whole fade at once.
        let mut elapsed = Duration::ZERO;
        while elapsed < Duration::from_secs(60) {
            app.update(tick);
            elapsed += tick;
        }
        assert!(app.transition.is_some(), "no fade started");
        assert_eq!(app.animation.name(), first, "swapped too early");

        // The new animation takes over at the halfway point.
        for _ in 0..14 {
            app.update(tick);
        }
        assert_ne!(app.animation.name(), first, "never swapped");

        for _ in 0..20 {
            app.update(tick);
        }
        assert!(app.transition.is_none(), "the fade never finished");
        assert_eq!(app.transition_keep(), 1.0);
    }

    #[test]
    fn a_fade_is_darkest_in_its_middle() {
        let mut app = app_from(&["--cycle-minutes", "1", "--seed", "1"]);
        assert_eq!(app.transition_keep(), 1.0, "no fade, full brightness");

        app.start_switch(0);
        assert!(app.transition_keep() > 0.9, "a fade starts lit");
        app.advance_transition(TRANSITION / 2);
        assert!(app.transition_keep() < 0.1, "a fade should dip to black");
        app.advance_transition(TRANSITION / 2);
        assert_eq!(app.transition_keep(), 1.0, "and come back lit");
    }

    #[test]
    fn switching_never_picks_the_same_animation() {
        let mut app = app_from(&["--seed", "3"]);
        for _ in 0..200 {
            let before = app.anim_index;
            let next = app.other_animation();
            assert_ne!(next, before, "cycled onto the same animation");
            app.anim_index = next;
        }
    }

    #[test]
    fn a_resize_reaches_the_animation_waiting_to_fade_in() {
        let mut app = app_from(&["--seed", "1"]);
        app.resize(Rect::new(0, 0, 40, 12));
        let next = app.other_animation();
        app.start_switch(next);

        let bigger = Rect::new(0, 0, 100, 30);
        app.resize(bigger);
        // Finish the fade, then draw: a stale layout would panic or come out
        // clipped to the old size.
        app.update(TRANSITION);
        let rows = frame(&mut app, 100, 30);
        assert_eq!(rows.len(), 30);
    }

    #[test]
    fn draws_the_animation_behind_the_clock() {
        let mut app = app_from(&["--anim", "starfield", "--seed", "11"]);
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

        let mut bare = app_from(&["--anim", "starfield", "--seed", "11", "--no-backdrop"]);
        let lit = frame_buffer(&mut bare, width, height);
        let mut shaded = app_from(&["--anim", "starfield", "--seed", "11"]);
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
        let mut bare = app_from(&["--anim", "starfield", "--seed", "11", "--no-backdrop"]);
        let mut shaded = app_from(&["--anim", "starfield", "--seed", "11"]);
        assert_eq!(frame(&mut bare, 80, 24), frame(&mut shaded, 80, 24));
    }

    #[test]
    fn fps_counter_is_off_by_default_and_on_with_the_flag() {
        let mut off = app_from(&["--seed", "3"]);
        assert!(!frame(&mut off, 80, 24).join("\n").contains("fps"));

        let mut on = app_from(&["--seed", "3", "--show-fps"]);
        assert!(frame(&mut on, 80, 24)[0].trim_end().ends_with("fps"));
    }

    fn key_code(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    /// An app in preview mode, past the grace period.
    fn preview() -> App {
        let mut app = app_from(&["--preview", "--anim", "starfield", "--seed", "1"]);
        app.resize(Rect::new(0, 0, 80, 24));
        app.update(GRACE_PERIOD);
        app
    }

    #[test]
    fn preview_ignores_keys_that_mean_nothing() {
        let mut app = preview();
        for code in [KeyCode::Char('x'), KeyCode::Enter, KeyCode::Char(' ')] {
            app.handle_event(&key_code(code));
            assert!(app.running, "{code:?} should not quit in preview mode");
        }
    }

    #[test]
    fn preview_quits_on_q_and_escape() {
        for code in [KeyCode::Char('q'), KeyCode::Char('Q'), KeyCode::Esc] {
            let mut app = preview();
            app.handle_event(&key_code(code));
            assert!(!app.running, "{code:?} should quit");
        }
    }

    #[test]
    fn preview_steps_through_the_animations() {
        let names = crate::anim::all_names();
        let mut app = preview();
        let start = app.anim_index;

        app.handle_event(&key_code(KeyCode::Char('n')));
        assert_eq!(app.anim_index, (start + 1) % names.len());

        app.handle_event(&key_code(KeyCode::Char('p')));
        app.handle_event(&key_code(KeyCode::Char('p')));
        assert_eq!(app.anim_index, (start + names.len() - 1) % names.len());

        // And the change actually lands once the fade reaches its middle.
        app.update(TRANSITION);
        assert_eq!(app.animation.name(), names[app.anim_index]);
    }

    #[test]
    fn preview_steps_through_the_themes() {
        let names = Theme::all_names();
        let mut app = preview();
        let start = app.theme_index;

        for step in 1..=names.len() {
            app.handle_event(&key_code(KeyCode::Char('t')));
            let expected = (start + step) % names.len();
            assert_eq!(app.theme_index, expected);
            assert_eq!(app.theme.name(), names[expected]);
        }
        assert_eq!(app.theme_index, start, "the themes should wrap round");
    }

    #[test]
    fn preview_steps_through_the_clock_positions() {
        let names = ClockPosition::all_names();
        let mut app = preview();
        let start = app.motion.position().name();
        let index = names.iter().position(|n| *n == start).unwrap();

        for step in 1..=names.len() {
            app.handle_event(&key_code(KeyCode::Char('c')));
            assert_eq!(
                app.motion.position().name(),
                names[(index + step) % names.len()]
            );
        }
        assert_eq!(app.motion.position().name(), start, "should wrap round");
    }

    #[test]
    fn leaving_bounce_restores_the_theme_colour() {
        let mut app = app_from(&["--preview", "--clock-position", "bounce", "--seed", "1"]);
        app.resize(Rect::new(0, 0, 80, 24));
        app.update(GRACE_PERIOD);
        // Run far enough to hit an edge and take an accent colour.
        for _ in 0..400 {
            app.update(Duration::from_millis(33));
        }
        assert!(app.motion.bounces() > 0, "never hit an edge");

        app.handle_event(&key_code(KeyCode::Char('c')));
        assert_ne!(app.motion.position(), ClockPosition::Bounce);
        assert_eq!(app.clock.digit_color, app.theme.clock_color());
    }

    #[test]
    fn preview_shows_the_keys_and_the_status() {
        let mut app = preview();
        let rows = frame(&mut app, 80, 24);
        assert!(rows[23].contains("q quit"), "no key hint: {:?}", rows[23]);
        assert!(rows[0].contains("starfield"), "no status: {:?}", rows[0]);
    }

    #[test]
    fn outside_preview_every_key_still_quits() {
        for code in [KeyCode::Char('n'), KeyCode::Char('t'), KeyCode::Enter] {
            let mut app = app_from(&["--anim", "starfield"]);
            app.update(GRACE_PERIOD);
            app.handle_event(&key_code(code));
            assert!(!app.running, "{code:?} should quit outside preview mode");
        }
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
