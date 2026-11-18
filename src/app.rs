//! Application state and the fixed-rate main loop.

use std::{
    fmt::Write as _,
    path::PathBuf,
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
    color::{self, ColorDepth},
    config::Config,
    menu::{self, KeyBar, Menu, Setting},
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

/// Frames run before `--bench` starts its clock, to keep one-off allocations
/// and cold caches out of the measurement.
const BENCH_WARMUP_FRAMES: u32 = 200;

/// How long a change of animation takes, from lit through black and back.
const TRANSITION: Duration = Duration::from_millis(900);

/// How long a message such as "saved" stays on screen.
const NOTICE_TIME: Duration = Duration::from_millis(2500);

/// What a key press means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The default: the keys along the bottom open settings lists, and a
    /// change is written back to the config file.
    Settings,
    /// A plain screensaver: any key or mouse movement quits.
    Lock,
}

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
    /// How much colour this terminal takes, worked out once at startup.
    color: ColorDepth,
    /// Whether to darken the animation behind the clock.
    backdrop: bool,
    /// Randomness for picking the next animation and seeding it.
    rng: StdRng,
    /// The animations this run draws from, in registry order.
    anim_pool: Vec<&'static str>,
    /// Index of the running animation in `anim_pool`.
    anim_index: usize,
    /// How long to wait between animation changes, or `None` for never.
    cycle_interval: Option<Duration>,
    /// Time since the last change of animation.
    since_switch: Duration,
    /// The fade in progress, if any.
    transition: Option<Transition>,
    /// Everything the file and the command line agreed on, kept live so a
    /// change can be written straight back out.
    settings: Config,
    /// Where a change is saved, or `None` on a platform with nowhere to put
    /// it.
    config_path: Option<PathBuf>,
    /// Whether keys change settings or simply end the screensaver.
    mode: Mode,
    /// The list currently open, if any.
    menu: Option<Menu>,
    /// The key reminder along the bottom.
    key_bar: KeyBar,
    /// A short message under the key bar, such as "ayarlar kaydedildi".
    notice: String,
    /// How long that message has left.
    notice_left: Duration,
    /// Whether the animation may change on its own; `--preview` turns this
    /// off so browsing is not interrupted.
    auto_cycle: bool,
    /// Whether to draw the status and frames-per-second readout.
    show_fps: bool,
    /// Smoothed frames per second, once there has been a frame to measure.
    fps: Option<f32>,
    /// Pre-rendered counter text, so drawing never formats or allocates.
    fps_text: String,
}

impl App {
    /// Build an app from the merged settings.
    ///
    /// `config` carries everything the file and the command line agreed on;
    /// `cli` is still needed for the flags that only make sense for one run,
    /// such as preview mode.
    pub fn new(cli: &Cli, config: &Config) -> color_eyre::Result<Self> {
        let mut rng = StdRng::seed_from_u64(config.resolved_seed());

        let anim_pool = config.animation_pool();
        if anim_pool.is_empty() {
            return Err(eyre!("no animations to draw from"));
        }
        // One animation is a pin; several mean start somewhere random.
        let anim_index = if anim_pool.len() == 1 {
            0
        } else {
            rng.random_range(0..anim_pool.len())
        };
        let animation = crate::anim::by_name(anim_pool[anim_index], rng.random::<u64>())
            .ok_or_else(|| eyre!("unknown animation `{}`; try --list", anim_pool[anim_index]))?;

        let theme = Theme::by_name(&config.theme)
            .ok_or_else(|| eyre!("unknown theme `{}`; try --list-themes", config.theme))?;

        let mut app = Self {
            running: true,
            frame_time: config.frame_time(),
            uptime: Duration::ZERO,
            clock: ClockWidget::new(),
            motion: ClockMotion::new(config.clock_position()),
            animation,
            area: Rect::ZERO,
            rng,
            anim_pool,
            anim_index,
            cycle_interval: None,
            since_switch: Duration::ZERO,
            transition: None,
            theme,
            color: config.color_setting().resolve(),
            backdrop: config.clock_backdrop,
            settings: config.clone(),
            config_path: Config::save_path(cli),
            mode: if cli.lock { Mode::Lock } else { Mode::Settings },
            menu: None,
            key_bar: KeyBar::new(),
            notice: String::new(),
            notice_left: Duration::ZERO,
            // Browsing by hand and changing on a timer would fight.
            auto_cycle: !cli.preview,
            // Preview mode needs the readout to say what is on screen.
            show_fps: cli.show_fps || cli.preview,
            fps: None,
            fps_text: String::with_capacity(48),
        };

        // Everything derived from the settings is built in one place, so that
        // starting up and changing a setting later cannot drift apart.
        app.apply_settings();
        Ok(app)
    }

    /// Rebuild everything that is derived from [`Self::settings`].
    ///
    /// Called once at startup and again after every change, so a setting
    /// only ever has to be written to `settings`.
    fn apply_settings(&mut self) {
        self.frame_time = self.settings.frame_time();
        self.color = self.settings.color_setting().resolve();
        self.backdrop = self.settings.clock_backdrop;

        // The clock caches the strings it draws, so changing what it shows
        // means building it again rather than poking at it.
        self.clock = ClockWidget::new()
            .with_format(
                self.settings.show_seconds,
                self.settings.show_date,
                self.settings.hour_format(),
            )
            .with_font(self.settings.clock_font(), self.settings.clock_scale);
        // Give it a time straight away, so its size can be measured on this
        // very frame instead of coming out empty.
        self.clock.update_time(Local::now().naive_local());

        if let Some(theme) = Theme::by_name(&self.settings.theme) {
            self.theme = theme;
        }
        let position = self.settings.clock_position();
        if position != self.motion.position() {
            self.motion.set_position(position);
        }
        self.clock.date_color = self.theme.date_color();
        if position == ClockPosition::Bounce {
            // Bounce paints the digits itself, from the theme's accents.
            self.clock.digit_color = self.theme.accent(self.motion.bounces());
        } else {
            self.clock.digit_color = self.theme.clock_color();
        }

        let pool = self.settings.animation_pool();
        if !pool.is_empty() {
            self.anim_pool = pool;
            self.anim_index = self
                .anim_pool
                .iter()
                .position(|name| *name == self.animation.name())
                .unwrap_or(0);
            if self.anim_pool[self.anim_index] != self.animation.name() {
                // Browsing a list has to show the effect at once; a 900 ms
                // fade would make the list feel broken.
                self.set_animation_now(self.anim_index);
            }
        }
        self.cycle_interval = if self.auto_cycle {
            self.settings.cycle_interval()
        } else {
            None
        };
    }

    /// Put the animation at `index` of the pool on screen without a fade.
    fn set_animation_now(&mut self, index: usize) {
        let index = index % self.anim_pool.len();
        let Some(mut animation) = crate::anim::by_name(self.anim_pool[index], self.rng.random())
        else {
            return;
        };
        animation.resize(self.area);
        self.animation = animation;
        self.anim_index = index;
        self.since_switch = Duration::ZERO;
        self.transition = None;
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

    /// Start fading over to the animation at `index` of the pool.
    fn start_switch(&mut self, index: usize) {
        let index = index % self.anim_pool.len();
        let name = self.anim_pool[index];
        let Some(mut next) = crate::anim::by_name(name, self.rng.random::<u64>()) else {
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

    /// A random animation from the pool that is not the one already running.
    fn other_animation(&mut self) -> usize {
        let count = self.anim_pool.len();
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
        let area = self.clock_area();
        self.motion.set_bounds(area, self.clock.measure(area));
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
        self.notice_left = self.notice_left.saturating_sub(dt);
        self.update_fps(dt);
    }

    /// The part of the screen the clock may use.
    ///
    /// In the settings mode the bottom row belongs to the key bar, so the
    /// clock is kept out of it rather than being drawn over.
    fn clock_area(&self) -> Rect {
        if self.mode == Mode::Settings && self.area.height > 1 {
            Rect {
                height: self.area.height - 1,
                ..self.area
            }
        } else {
            self.area
        }
    }

    /// Keep the smoothed frame rate and its pre-rendered text up to date.
    fn update_fps(&mut self, dt: Duration) {
        if !self.show_fps || dt.is_zero() {
            return;
        }
        let sample = 1.0 / dt.as_secs_f32();
        // The first frame has nothing to smooth against, so it is the estimate.
        let smoothed = match self.fps {
            Some(previous) => previous + (sample - previous) * FPS_SMOOTHING,
            None => sample,
        };
        self.fps = Some(smoothed);

        // The animation, theme and clock position ride along; they are the
        // other things worth seeing while debugging a frame rate problem.
        self.fps_text.clear();
        let _ = write!(
            self.fps_text,
            "{} · {} · {} · {:.0} fps",
            self.animation.name(),
            self.theme.name(),
            self.motion.position().name(),
            smoothed
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
        if self.mode == Mode::Settings {
            self.key_bar.render(area, buf);
            self.render_notice(area, buf);
            // Last of the overlays, so an open list sits on top of them all.
            if let Some(menu) = self.menu.as_ref() {
                menu.render(area, buf);
            }
        }

        // Last of all: everything above drew in truecolor, so this is the one
        // place that has to know what the terminal can take.
        color::reduce(buf, area, self.color);
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
    /// Outside preview mode any key press or mouse movement quits, once the
    /// grace period is over. Resize, focus and paste events never do.
    fn handle_event(&mut self, event: &Event) {
        if self.uptime < GRACE_PERIOD {
            return;
        }

        match event {
            // On Windows every key arrives twice (Press and Release); only
            // Press counts, or the program would quit as it started.
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key.code),
            // Someone reaching for the mouse is someone back at the machine.
            // In settings mode they are configuring, so it is left alone.
            Event::Mouse(_) if self.mode == Mode::Lock => self.running = false,
            _ => {}
        }
    }

    /// React to a key press.
    fn handle_key(&mut self, code: KeyCode) {
        // A screensaver gets out of the way; that is its whole job.
        if self.mode == Mode::Lock {
            self.running = false;
            return;
        }
        // An open list takes the keys first, so j, k and q mean something
        // different while one is up.
        if self.menu.is_some() {
            self.handle_menu_key(code);
            return;
        }

        let count = self.anim_pool.len();
        match code {
            KeyCode::Char('q' | 'Q') | KeyCode::Esc => self.running = false,
            KeyCode::Char('?') => self.menu = Some(Menu::help(self.area)),
            KeyCode::Char('n' | 'N') | KeyCode::Right => self.start_switch(self.anim_index + 1),
            KeyCode::Char('p' | 'P') | KeyCode::Left => {
                self.start_switch(self.anim_index + count - 1);
            }
            // Anything else is either a settings key or nothing at all.
            // Unlike the screensaver, a stray key must not quit: losing what
            // you were setting up to a mistyped letter would be rude.
            KeyCode::Char(key) => {
                if let Some(setting) = Setting::from_key(key.to_ascii_lowercase()) {
                    self.open_menu(setting);
                }
            }
            _ => {}
        }
    }

    /// Open the list for `setting`, on the value in force.
    fn open_menu(&mut self, setting: Setting) {
        let (items, index) = menu::choices(setting, &self.settings);
        self.menu = Some(Menu::new(setting, &items, index, self.area));
    }

    /// React to a key press while a list is open.
    fn handle_menu_key(&mut self, code: KeyCode) {
        let step = match code {
            KeyCode::Up | KeyCode::Char('k' | 'K') => Some(-1),
            KeyCode::Down | KeyCode::Char('j' | 'J') => Some(1),
            _ => None,
        };
        if let Some(step) = step {
            let area = self.area;
            if let Some(menu) = self.menu.as_mut() {
                menu.move_by(step, area);
            }
            // Show the highlighted value at once: picking a theme by eye is
            // the point of the list.
            self.show_selected();
            return;
        }

        match code {
            KeyCode::Enter => self.commit_menu(),
            KeyCode::Esc | KeyCode::Char('q' | 'Q') => self.cancel_menu(),
            _ => {}
        }
    }

    /// Apply whatever the open list has highlighted, without saving it.
    fn show_selected(&mut self) {
        // Owned, so the borrow of `self.menu` is over before `self` is
        // handed out mutably again.
        let picked = self.menu.as_ref().and_then(|menu| {
            let setting = menu.setting()?;
            Some((setting, menu.selected_value()?.to_owned()))
        });
        let Some((setting, value)) = picked else {
            return;
        };
        menu::apply(setting, &value, &mut self.settings);
        self.apply_settings();
    }

    /// Keep what the list has highlighted and write it to the config file.
    fn commit_menu(&mut self) {
        // Enter without having moved: nothing has been applied yet.
        self.show_selected();
        let changed = self.menu.take().and_then(|menu| menu.setting()).is_some();
        if changed {
            self.save_settings();
        }
    }

    /// Close the list and put back the value it opened on.
    fn cancel_menu(&mut self) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        if let (Some(setting), Some(value)) = (menu.setting(), menu.opened_value()) {
            menu::apply(setting, value, &mut self.settings);
        }
        self.apply_settings();
    }

    /// Write the live settings out, and say on screen how it went.
    fn save_settings(&mut self) {
        let Some(path) = self.config_path.clone() else {
            self.notify("ayar dosyası için bir yer bulunamadı".to_owned());
            return;
        };
        match self.settings.save(&path) {
            Ok(()) => self.notify("ayarlar kaydedildi".to_owned()),
            // There is no stderr to complain to from inside the alternate
            // screen, so the reason goes on screen.
            Err(error) => self.notify(format!("kaydedilemedi: {error}")),
        }
    }

    /// Put a message under the key bar for a few seconds.
    fn notify(&mut self, text: String) {
        self.notice = text;
        self.notice_left = NOTICE_TIME;
    }

    /// Draw that message, while it lasts.
    fn render_notice(&self, area: Rect, buf: &mut Buffer) {
        if self.notice_left.is_zero() || area.height < 2 {
            return;
        }
        let width = render::text_cols(&self.notice);
        if width == 0 || area.width < width {
            return;
        }
        let x = area.x + (area.width - width) / 2;
        let y = area.bottom() - 2;
        render::fade_to_black(buf, Rect::new(x, y, width, 1), OVERLAY_KEEP);
        buf.set_string(
            x,
            y,
            &self.notice,
            Style::new().fg(self.theme.clock_color()),
        );
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
pub fn dump_frame(cli: &Cli, config: &Config) -> color_eyre::Result<()> {
    let (width, height) = cli.dump_size();
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    let mut app = App::new(cli, config)?;

    app.resize(Rect::new(0, 0, width, height));
    // Simulate a few seconds so the picture is representative: effects that
    // fall in from off screen look empty on their first frame.
    for _ in 0..DUMP_WARMUP_FRAMES {
        app.update(config.frame_time());
    }
    terminal.draw(|frame| app.render(frame))?;

    print!("{}", buffer_to_text(terminal.backend().buffer()));
    Ok(())
}

/// Time `frames` worth of work off screen and print what it cost.
///
/// Uses the same headless backend as `--dump-frame`, so the real terminal is
/// never touched; what it leaves out is the write to the terminal itself.
pub fn bench(cli: &Cli, config: &Config, frames: u32) -> color_eyre::Result<()> {
    let (width, height) = cli.dump_size();
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    let mut app = App::new(cli, config)?;
    app.resize(Rect::new(0, 0, width, height));

    let step = config.frame_time();
    for _ in 0..BENCH_WARMUP_FRAMES {
        app.update(step);
        terminal.draw(|frame| app.render(frame))?;
    }

    let started = Instant::now();
    for _ in 0..frames {
        app.update(step);
        terminal.draw(|frame| app.render(frame))?;
    }
    let per_frame = started.elapsed().as_secs_f64() / f64::from(frames);

    println!(
        "{:<10} {:<8} {width}x{height}  {:7.1} us/frame  {:5.2}% of one core at {} fps",
        app.animation.name(),
        app.theme.name(),
        per_frame * 1e6,
        per_frame * f64::from(config.fps) * 100.0,
        config.fps,
    );
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
    use ratatui::crossterm::event::{
        KeyCode, KeyEvent, KeyEventState, KeyModifiers, MouseEvent, MouseEventKind,
    };

    /// Build an app from arguments alone.
    ///
    /// Deliberately starts from `Config::default()` rather than
    /// `Config::resolve`, so tests never depend on the config file that
    /// happens to be on the machine running them.
    ///
    /// The colour depth is pinned for the same reason: the default is `auto`,
    /// which reads COLORTERM and TERM, and a machine without them reduces
    /// every frame to no colour at all. Tests that are themselves about
    /// colour say so and are left alone.
    fn app_from(args: &[&str]) -> App {
        let mut full = vec!["binkam"];
        let picks_colour = args
            .iter()
            .any(|arg| *arg == "--color" || *arg == "--no-color");
        if !picks_colour {
            full.extend_from_slice(&["--color", "truecolor"]);
        }
        // Saving is a key press away, so every test is pointed at a
        // throwaway file. Without this, a test that presses Enter would
        // write the config file of whoever is running the suite.
        let scratch = crate::config::scratch_config();
        if !args.contains(&"--config") {
            full.extend_from_slice(&[
                "--config",
                scratch.to_str().expect("a utf-8 temporary path"),
            ]);
        }
        full.extend_from_slice(args);
        let cli = Cli::parse_from(full);
        let config = Config::merge(Config::default(), &cli).expect("valid settings");
        App::new(&cli, &config).expect("the default animation must exist")
    }

    fn app() -> App {
        app_from(&[])
    }

    /// A plain screensaver, which is what `--lock` asks for: any key quits.
    fn locked() -> App {
        app_from(&["--lock"])
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
        let mut app = locked();
        app.update(GRACE_PERIOD);
        app.handle_event(&key(KeyEventKind::Press));
        assert!(!app.running);
    }

    #[test]
    fn key_release_is_ignored() {
        let mut app = locked();
        app.update(GRACE_PERIOD);
        app.handle_event(&key(KeyEventKind::Release));
        assert!(app.running, "Windows sends Release after every Press");
    }

    #[test]
    fn events_during_grace_period_are_ignored() {
        let mut app = locked();
        app.update(Duration::from_millis(100));
        app.handle_event(&key(KeyEventKind::Press));
        assert!(app.running);
    }

    fn mouse_moved() -> Event {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 4,
            row: 2,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn mouse_movement_quits() {
        let mut app = locked();
        app.update(GRACE_PERIOD);
        app.handle_event(&mouse_moved());
        assert!(!app.running, "a screensaver should get out of the way");
    }

    #[test]
    fn mouse_movement_during_the_grace_period_is_ignored() {
        let mut app = locked();
        app.update(Duration::from_millis(100));
        app.handle_event(&mouse_moved());
        assert!(app.running);
    }

    #[test]
    fn the_settings_mode_ignores_the_mouse() {
        let mut app = preview();
        app.handle_event(&mouse_moved());
        assert!(app.running, "configuring should not be interrupted");
    }

    #[test]
    fn resize_does_not_quit() {
        let mut app = locked();
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
        let cli = Cli::parse_from(["binkam"]);
        let config = Config {
            anim: vec!["nope".to_owned()],
            ..Config::default()
        };
        assert!(App::new(&cli, &config).is_err());
    }

    #[test]
    fn a_pool_of_one_pins_the_animation() {
        let app = app_from(&["--anim", "fire"]);
        assert_eq!(app.anim_pool, ["fire"]);
        assert_eq!(app.anim_index, 0);
        assert_eq!(app.cycle_interval, None);
    }

    #[test]
    fn a_pool_of_two_cycles_between_just_those() {
        let mut app = app_from(&["--anim", "fire", "--anim", "snow", "--seed", "1"]);
        assert_eq!(app.anim_pool, ["fire", "snow"]);
        assert!(app.cycle_interval.is_some());

        for _ in 0..50 {
            let next = app.other_animation();
            assert!(next < 2, "stepped outside the pool");
            app.anim_index = next;
        }
    }

    #[test]
    fn preview_never_cycles_on_a_timer() {
        let app = app_from(&["--preview", "--cycle-minutes", "1"]);
        assert_eq!(app.cycle_interval, None);
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

        // Only the animation uses Rgb, and only truecolor keeps it that way,
        // which `app_from` pins for exactly this reason.
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

    /// An app in the settings mode — the default — past the grace period.
    fn settings() -> App {
        let mut app = app_from(&["--anim", "starfield", "--seed", "1"]);
        app.resize(Rect::new(0, 0, 80, 24));
        app.update(GRACE_PERIOD);
        app
    }

    /// The same, with `--preview`: the status readout and no timer.
    fn preview() -> App {
        let mut app = app_from(&["--preview", "--anim", "starfield", "--seed", "1"]);
        app.resize(Rect::new(0, 0, 80, 24));
        app.update(GRACE_PERIOD);
        app
    }

    /// Which setting the open list changes, or `None` for the key help.
    fn open_setting(app: &App) -> Option<Setting> {
        app.menu.as_ref().and_then(Menu::setting)
    }

    #[test]
    fn the_settings_mode_is_the_default_and_lock_turns_it_off() {
        assert_eq!(app_from(&[]).mode, Mode::Settings);
        assert_eq!(app_from(&["--preview"]).mode, Mode::Settings);
        assert_eq!(app_from(&["--lock"]).mode, Mode::Lock);
    }

    #[test]
    fn under_lock_every_key_quits() {
        for code in [
            KeyCode::Char('n'),
            KeyCode::Char('t'),
            KeyCode::Char('?'),
            KeyCode::Enter,
        ] {
            let mut app = app_from(&["--lock", "--anim", "starfield"]);
            app.update(GRACE_PERIOD);
            app.handle_event(&key_code(code));
            assert!(!app.running, "{code:?} should quit a screensaver");
        }
    }

    #[test]
    fn a_stray_key_does_not_quit_the_settings_mode() {
        // Losing what you were setting up to a mistyped letter would be rude.
        let mut app = settings();
        for code in [
            KeyCode::Char('x'),
            KeyCode::Enter,
            KeyCode::Char(' '),
            KeyCode::Char('9'),
        ] {
            app.handle_event(&key_code(code));
            assert!(app.running, "{code:?} should not quit");
            assert!(app.menu.is_none(), "{code:?} should not open a list");
        }
    }

    #[test]
    fn the_settings_mode_quits_on_q_and_escape() {
        for code in [KeyCode::Char('q'), KeyCode::Char('Q'), KeyCode::Esc] {
            let mut app = settings();
            app.handle_event(&key_code(code));
            assert!(!app.running, "{code:?} should quit");
        }
    }

    #[test]
    fn n_and_p_still_step_through_the_animations() {
        let mut app = app_from(&["--seed", "1"]);
        app.resize(Rect::new(0, 0, 80, 24));
        app.update(GRACE_PERIOD);
        let names = app.anim_pool.clone();
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
    fn every_setting_has_a_key_that_opens_its_list() {
        for setting in Setting::ALL {
            let mut app = settings();
            app.handle_event(&key_code(KeyCode::Char(setting.key())));
            assert_eq!(open_setting(&app), Some(setting), "{setting:?}");
        }
    }

    #[test]
    fn browsing_the_theme_list_changes_the_theme_at_once() {
        let mut app = settings();
        let before = app.theme.name();

        app.handle_event(&key_code(KeyCode::Char('t')));
        app.handle_event(&key_code(KeyCode::Down));
        assert_ne!(
            app.theme.name(),
            before,
            "the theme did not follow the list"
        );
        assert_eq!(
            app.settings.theme,
            app.theme.name(),
            "the settings and the screen disagree"
        );
        assert_eq!(app.clock.date_color, app.theme.date_color());
    }

    #[test]
    fn escape_puts_back_the_value_the_list_opened_on() {
        let mut app = settings();
        let before = app.theme.name().to_owned();

        app.handle_event(&key_code(KeyCode::Char('t')));
        app.handle_event(&key_code(KeyCode::Down));
        app.handle_event(&key_code(KeyCode::Down));
        app.handle_event(&key_code(KeyCode::Esc));

        assert!(app.menu.is_none(), "Esc should close the list");
        assert!(app.running, "Esc inside a list should not quit");
        assert_eq!(app.theme.name(), before);
        assert_eq!(app.settings.theme, before);
    }

    #[test]
    fn q_closes_an_open_list_instead_of_quitting() {
        let mut app = settings();
        app.handle_event(&key_code(KeyCode::Char('t')));
        app.handle_event(&key_code(KeyCode::Char('q')));
        assert!(
            app.running,
            "q should have closed the list, not the program"
        );
        assert!(app.menu.is_none());

        // And again, with nothing open, it quits.
        app.handle_event(&key_code(KeyCode::Char('q')));
        assert!(!app.running);
    }

    #[test]
    fn enter_keeps_the_choice_and_writes_it_to_the_file() {
        let path = crate::config::scratch_path("config.toml");
        let name = path.to_str().expect("a utf-8 temporary path");
        let mut app = app_from(&["--config", name, "--anim", "starfield", "--seed", "1"]);
        app.resize(Rect::new(0, 0, 80, 24));
        app.update(GRACE_PERIOD);

        app.handle_event(&key_code(KeyCode::Char('t')));
        app.handle_event(&key_code(KeyCode::Down));
        let chosen = app.theme.name().to_owned();
        app.handle_event(&key_code(KeyCode::Enter));

        assert!(app.menu.is_none(), "Enter should close the list");
        let saved = Config::from_file(&path).expect("Enter should have written a file");
        assert_eq!(saved.theme, chosen, "the file disagrees with the screen");
        assert!(
            app.notice.contains("kaydedildi"),
            "no word of the save: {:?}",
            app.notice
        );

        let _ = std::fs::remove_dir_all(path.parent().expect("a parent directory"));
    }

    #[test]
    fn browsing_alone_never_touches_the_file() {
        let path = crate::config::scratch_path("config.toml");
        let name = path.to_str().expect("a utf-8 temporary path");
        let mut app = app_from(&["--config", name, "--anim", "starfield", "--seed", "1"]);
        app.update(GRACE_PERIOD);

        app.handle_event(&key_code(KeyCode::Char('t')));
        app.handle_event(&key_code(KeyCode::Down));
        app.handle_event(&key_code(KeyCode::Esc));
        assert!(!path.exists(), "{path:?} should not have been written");
    }

    #[test]
    fn picking_an_animation_pins_it_and_shows_it_at_once() {
        let mut app = settings();
        assert_eq!(app.settings.anim, ["starfield"]);

        app.handle_event(&key_code(KeyCode::Char('a')));
        app.handle_event(&key_code(KeyCode::Down));

        let name = app.animation.name();
        assert_eq!(app.settings.anim, [name], "the choice was not pinned");
        assert_ne!(name, "starfield", "nothing changed");
        assert!(app.transition.is_none(), "browsing should not fade");
        assert_eq!(app.cycle_interval, None, "a pinned animation cannot cycle");
    }

    #[test]
    fn choosing_all_of_them_starts_the_timer_again() {
        let mut app = settings();
        assert_eq!(app.cycle_interval, None, "one animation cannot cycle");

        app.handle_event(&key_code(KeyCode::Char('a')));
        // The first row of the animation list is "all of them, in turn".
        app.handle_event(&key_code(KeyCode::Up));
        app.handle_event(&key_code(KeyCode::Down));
        while app.settings.anim.len() == 1 {
            app.handle_event(&key_code(KeyCode::Up));
        }
        assert!(app.settings.anim.is_empty());
        assert_eq!(app.anim_pool.len(), crate::anim::all_names().len());
        assert!(app.cycle_interval.is_some(), "the timer should be back");
    }

    #[test]
    fn changing_the_clock_font_and_size_reaches_the_clock() {
        let mut app = settings();
        app.handle_event(&key_code(KeyCode::Char('f')));
        app.handle_event(&key_code(KeyCode::Down));
        assert_eq!(app.clock.font().name(), app.settings.clock_font);
        assert_ne!(app.clock.font().name(), "block", "the font did not change");

        app.handle_event(&key_code(KeyCode::Enter));
        app.handle_event(&key_code(KeyCode::Char('b')));
        app.handle_event(&key_code(KeyCode::Down));
        assert_eq!(app.settings.clock_scale, 2);
        assert_eq!(app.clock.scale(), 2);
    }

    #[test]
    fn turning_the_seconds_off_shortens_the_time() {
        let mut app = settings();
        assert_eq!(app.clock.time_text().len(), 8, "HH:MM:SS to start with");

        app.handle_event(&key_code(KeyCode::Char('s')));
        // "açık" is the first row, so one step down is "kapalı".
        app.handle_event(&key_code(KeyCode::Down));
        app.update(Duration::from_millis(33));
        assert!(!app.settings.show_seconds);
        assert_eq!(app.clock.time_text().len(), 5, "HH:MM from now on");
    }

    #[test]
    fn leaving_bounce_restores_the_theme_colour() {
        let mut app = app_from(&["--clock-position", "bounce", "--seed", "1"]);
        app.resize(Rect::new(0, 0, 80, 24));
        app.update(GRACE_PERIOD);
        // Run far enough to hit an edge and take an accent colour.
        for _ in 0..400 {
            app.update(Duration::from_millis(33));
        }
        assert!(app.motion.bounces() > 0, "never hit an edge");

        // `k` opens the position list, which opens on bounce; one down.
        app.handle_event(&key_code(KeyCode::Char('k')));
        app.handle_event(&key_code(KeyCode::Down));
        assert_ne!(app.motion.position(), ClockPosition::Bounce);
        assert_eq!(app.clock.digit_color, app.theme.clock_color());
    }

    #[test]
    fn the_help_key_opens_a_list_that_changes_nothing() {
        let mut app = settings();
        app.handle_event(&key_code(KeyCode::Char('?')));
        assert!(app.menu.is_some(), "? opened nothing");
        assert_eq!(open_setting(&app), None, "the help changes no setting");

        let before = app.settings.clone();
        app.handle_event(&key_code(KeyCode::Down));
        assert_eq!(app.settings, before);
        app.handle_event(&key_code(KeyCode::Enter));
        assert!(app.menu.is_none(), "Enter should close the help");
        assert_eq!(app.settings, before);
    }

    #[test]
    fn the_key_bar_is_drawn_in_the_settings_mode_and_not_under_lock() {
        let mut app = settings();
        let rows = frame(&mut app, 80, 24);
        assert!(rows[23].contains("q çık"), "no key bar: {:?}", rows[23]);

        let mut bare = app_from(&["--lock", "--anim", "starfield", "--seed", "1"]);
        let rows = frame(&mut bare, 80, 24);
        assert!(
            !rows[23].contains("çık"),
            "a screensaver should be bare: {:?}",
            rows[23]
        );
    }

    #[test]
    fn the_key_bar_has_the_bottom_row_to_itself() {
        // On a short terminal the date line used to land exactly where the
        // bar goes, and the bar was drawn straight over it.
        for (width, height) in [(40, 8), (47, 7), (60, 9), (80, 24)] {
            let mut app = settings();
            let rows = frame(&mut app, width, height);
            let clock = app.motion.rect();
            assert!(
                clock.bottom() < height,
                "the clock reached the bar's row at {width}x{height}"
            );

            let bar = app.key_bar.text(width).expect("a bar should fit this");
            let bottom = &rows[usize::from(height) - 1];
            assert!(
                bottom.contains(bar),
                "the bar is not intact at {width}x{height}: {bottom:?}"
            );
        }
    }

    #[test]
    fn the_screensaver_keeps_the_whole_screen() {
        // Nothing is reserved without the bar, so the clock may use it all.
        let mut app = app_from(&["--lock", "--anim", "starfield", "--seed", "1"]);
        let _ = frame(&mut app, 40, 8);
        assert_eq!(app.clock_area(), Rect::new(0, 0, 40, 8));
    }

    #[test]
    fn preview_shows_the_status_readout() {
        let mut app = preview();
        let rows = frame(&mut app, 80, 24);
        assert!(rows[0].contains("starfield"), "no status: {:?}", rows[0]);
    }

    #[test]
    fn an_open_list_is_drawn_in_the_top_left_corner() {
        let mut app = settings();
        app.handle_event(&key_code(KeyCode::Char('t')));
        let rows = frame(&mut app, 80, 24);

        assert!(rows[1].starts_with(" ┌"), "no panel: {:?}", rows[1]);
        assert!(rows[1].contains("Tema"), "no heading: {:?}", rows[1]);
        assert!(
            rows.iter().any(|row| row.contains('›')),
            "nothing is highlighted"
        );
    }

    #[test]
    fn a_saved_setting_is_what_the_next_run_starts_from() {
        let path = crate::config::scratch_path("config.toml");
        let name = path.to_str().expect("a utf-8 temporary path");

        let mut app = app_from(&["--config", name, "--anim", "starfield", "--seed", "1"]);
        app.resize(Rect::new(0, 0, 80, 24));
        app.update(GRACE_PERIOD);
        app.handle_event(&key_code(KeyCode::Char('f')));
        app.handle_event(&key_code(KeyCode::Down));
        app.handle_event(&key_code(KeyCode::Enter));
        let chosen = app.settings.clock_font.clone();

        // What a second run would do: read the file, then start from it.
        let cli = Cli::parse_from(["binkam", "--config", name, "--color", "truecolor"]);
        let config = Config::resolve(&cli).expect("the saved file should be valid");
        let next = App::new(&cli, &config).expect("and should start up");
        assert_eq!(next.clock.font().name(), chosen);

        let _ = std::fs::remove_dir_all(path.parent().expect("a parent directory"));
    }

    #[test]
    fn colour_is_reduced_before_the_frame_leaves() {
        let mut indexed = app_from(&["--anim", "fire", "--color", "256", "--seed", "1"]);
        let buf = frame_buffer(&mut indexed, 60, 16);
        let colors: Vec<Color> = (0..16)
            .flat_map(|y| (0..60).map(move |x| (x, y)))
            .map(|(x, y)| buf[(x, y)].fg)
            .collect();
        assert!(
            colors.iter().any(|c| matches!(c, Color::Indexed(_))),
            "nothing was quantized"
        );
        assert!(
            !colors.iter().any(|c| matches!(c, Color::Rgb(..))),
            "truecolor escaped into a 256 colour frame"
        );

        let mut plain = app_from(&["--anim", "fire", "--no-color", "--seed", "1"]);
        let buf = frame_buffer(&mut plain, 60, 16);
        for y in 0..16 {
            for x in 0..60 {
                assert!(
                    !matches!(buf[(x, y)].fg, Color::Rgb(..) | Color::Indexed(_)),
                    "colour left at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn a_frame_without_colour_still_has_a_clock_and_a_fire() {
        let mut app = app_from(&["--anim", "fire", "--no-color", "--seed", "1"]);
        app.resize(Rect::new(0, 0, 80, 24));
        // The fire needs a moment to grow past its bottom row.
        for _ in 0..90 {
            app.update(Duration::from_millis(33));
        }
        let rows = frame(&mut app, 80, 24);
        let text = rows.join("\n");
        assert!(text.contains('█'), "the clock went missing");
        assert!(
            text.contains('▀') || text.contains('▄'),
            "the fire lost its shape: {text}"
        );
    }

    /// Sizes worth being rude with: degenerate, exactly on a fallback
    /// boundary, and far larger than any real terminal.
    const AWKWARD_SIZES: [(u16, u16); 12] = [
        (1, 1),
        (2, 1),
        (1, 2),
        (3, 2),
        (5, 1),
        (20, 6),
        (30, 5),
        (47, 5),
        (47, 7),
        (80, 24),
        (300, 2),
        (300, 100),
    ];

    #[test]
    fn every_animation_survives_every_awkward_size() {
        for name in crate::anim::all_names() {
            let mut app = app_from(&["--anim", name, "--seed", "1", "--show-fps"]);
            for (width, height) in AWKWARD_SIZES {
                let rows = frame(&mut app, width, height);
                assert_eq!(
                    rows.len(),
                    usize::from(height),
                    "{name} at {width}x{height}"
                );
                assert!(
                    rows.iter()
                        .all(|row| row.chars().count() == usize::from(width)),
                    "{name} at {width}x{height} drew a ragged frame"
                );
            }
        }
    }

    #[test]
    fn every_clock_position_survives_every_awkward_size() {
        for position in ClockPosition::all_names() {
            let mut app = app_from(&[
                "--clock-position",
                position,
                "--anim",
                "starfield",
                "--seed",
                "1",
            ]);
            for (width, height) in AWKWARD_SIZES {
                let rows = frame(&mut app, width, height);
                assert_eq!(
                    rows.len(),
                    usize::from(height),
                    "{position} at {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn resizing_on_every_frame_is_survivable() {
        // Someone dragging a window corner, with the clock and a changing
        // animation both having to keep up.
        let mut app = app_from(&["--cycle-minutes", "1", "--seed", "2"]);
        for round in 0..8 {
            for (width, height) in AWKWARD_SIZES {
                let rows = frame(&mut app, width, height);
                assert_eq!(
                    rows.len(),
                    usize::from(height),
                    "round {round} at {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn a_resize_in_the_middle_of_a_fade_is_survivable() {
        let mut app = app_from(&["--seed", "4"]);
        app.resize(Rect::new(0, 0, 80, 24));
        let next = app.other_animation();
        app.start_switch(next);

        // Resize on every frame of the fade, including right at the midpoint.
        for (width, height) in AWKWARD_SIZES {
            app.update(TRANSITION / 6);
            let rows = frame(&mut app, width, height);
            assert_eq!(rows.len(), usize::from(height));
        }
    }

    #[test]
    fn every_setting_list_survives_every_awkward_size() {
        for setting in Setting::ALL {
            let mut app = settings();
            app.handle_event(&key_code(KeyCode::Char(setting.key())));
            for (width, height) in AWKWARD_SIZES {
                let rows = frame(&mut app, width, height);
                assert_eq!(
                    rows.len(),
                    usize::from(height),
                    "{setting:?} at {width}x{height}"
                );
                assert!(
                    rows.iter()
                        .all(|row| row.chars().count() == usize::from(width)),
                    "{setting:?} drew a ragged frame at {width}x{height}"
                );
                // Move inside the list too: scrolling is worked out against
                // whatever the screen is now.
                app.handle_event(&key_code(KeyCode::Down));
            }
        }
    }

    #[test]
    fn the_key_help_survives_every_awkward_size() {
        let mut app = settings();
        app.handle_event(&key_code(KeyCode::Char('?')));
        for (width, height) in AWKWARD_SIZES {
            let rows = frame(&mut app, width, height);
            assert_eq!(rows.len(), usize::from(height), "{width}x{height}");
            app.handle_event(&key_code(KeyCode::Down));
        }
    }

    #[test]
    fn every_clock_font_and_size_survives_every_awkward_size() {
        for name in crate::clock::font::Font::all_names() {
            for scale in ["1", "4"] {
                let mut app = app_from(&[
                    "--clock-font",
                    name,
                    "--clock-scale",
                    scale,
                    "--anim",
                    "starfield",
                    "--seed",
                    "1",
                ]);
                for (width, height) in AWKWARD_SIZES {
                    let rows = frame(&mut app, width, height);
                    assert_eq!(
                        rows.len(),
                        usize::from(height),
                        "{name} at {scale}x on {width}x{height}"
                    );
                    assert!(
                        rows.iter()
                            .all(|row| row.chars().count() == usize::from(width)),
                        "{name} at {scale}x drew a ragged frame on {width}x{height}"
                    );
                }
            }
        }
    }

    #[test]
    fn dump_renders_requested_size() {
        let cli = Cli::parse_from([
            "binkam",
            "--dump-frame",
            "--size",
            "20x5",
            "--color",
            "truecolor",
        ]);
        let config = Config::merge(Config::default(), &cli).unwrap();
        let (width, height) = cli.dump_size();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let app = App::new(&cli, &config).unwrap();
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
