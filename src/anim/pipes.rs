//! Growing pipes that leave a permanent trail, like the old screensaver.
//!
//! Every step each pipe stamps a box-drawing glyph into the cell it is
//! leaving, picked from the direction it arrived in and the direction it
//! takes next, then moves on. The trail is never erased until the screen is
//! busy enough, at which point everything starts over.

use std::time::Duration;

use rand::{RngExt, SeedableRng, rngs::StdRng};
use ratatui::{buffer::Buffer, layout::Rect};

use super::Animation;
use crate::render::{Rgb, rgb};

/// Cells advanced per second by every pipe.
const STEPS_PER_SEC: f32 = 13.0;

/// Chance in a hundred that a pipe turns rather than carries straight on.
const TURN_PERCENT: u32 = 20;

/// One pipe per this many cells.
const CELLS_PER_PIPE: u32 = 620;

/// Pipe count bounds.
const MIN_PIPES: u32 = 2;
const MAX_PIPES: u32 = 8;

/// Share of the screen that has to be covered before it is wiped, in percent.
const FILL_PERCENT: usize = 45;

/// Longest frame gap the animation reacts to, which also bounds the work
/// done for any single frame.
const MAX_STEP: f32 = 0.1;

/// Colours a new pipe picks from.
const COLORS: [Rgb; 6] = [
    (96, 196, 255),
    (255, 148, 122),
    (150, 230, 160),
    (236, 196, 108),
    (198, 150, 246),
    (120, 232, 226),
];

/// A direction of travel on the cell grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Up,
    Right,
    Down,
    Left,
}

impl Dir {
    /// Every direction, for picking one at random.
    const ALL: [Self; 4] = [Self::Up, Self::Right, Self::Down, Self::Left];

    /// The direction facing the other way.
    fn opposite(self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Right => Self::Left,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
        }
    }

    /// A quarter turn anticlockwise.
    fn turn_left(self) -> Self {
        match self {
            Self::Up => Self::Left,
            Self::Left => Self::Down,
            Self::Down => Self::Right,
            Self::Right => Self::Up,
        }
    }

    /// A quarter turn clockwise.
    fn turn_right(self) -> Self {
        self.turn_left().opposite()
    }

    /// Column and row offset of one step in this direction.
    fn delta(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::Right => (1, 0),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
        }
    }
}

/// The glyph for a cell a pipe enters travelling `incoming` and leaves
/// travelling `outgoing`.
///
/// What the glyph has to connect are the side the pipe came in through,
/// which is `incoming.opposite()`, and the side it leaves by, `outgoing`.
fn glyph_for(incoming: Dir, outgoing: Dir) -> char {
    match (incoming.opposite(), outgoing) {
        (Dir::Up, Dir::Down) | (Dir::Down, Dir::Up) => '│',
        (Dir::Left, Dir::Right) | (Dir::Right, Dir::Left) => '─',
        (Dir::Up, Dir::Right) | (Dir::Right, Dir::Up) => '└',
        (Dir::Up, Dir::Left) | (Dir::Left, Dir::Up) => '┘',
        (Dir::Down, Dir::Right) | (Dir::Right, Dir::Down) => '┌',
        (Dir::Down, Dir::Left) | (Dir::Left, Dir::Down) => '┐',
        // Only reachable if a pipe doubled back, which it never does.
        _ => '┼',
    }
}

/// One growing pipe.
#[derive(Debug, Clone, Copy)]
struct Pipe {
    x: u16,
    y: u16,
    /// The direction it is currently travelling.
    dir: Dir,
    color: Rgb,
}

impl Pipe {
    /// A pipe starting at a random cell, heading in a random direction.
    fn spawn(rng: &mut StdRng, width: u16, height: u16) -> Self {
        Self {
            x: rng.random_range(0..width.max(1)),
            y: rng.random_range(0..height.max(1)),
            dir: Dir::ALL[rng.random_range(0..Dir::ALL.len())],
            color: COLORS[rng.random_range(0..COLORS.len())],
        }
    }
}

/// The pipes animation.
#[derive(Debug)]
pub struct Pipes {
    rng: StdRng,
    pipes: Vec<Pipe>,
    /// The trail: a glyph and colour per cell, `width * height` long.
    canvas: Vec<Option<(char, Rgb)>>,
    width: u16,
    height: u16,
    /// How many cells of the canvas are taken, so coverage needs no counting.
    filled: usize,
    /// Simulation time not yet consumed by a step.
    pending: f32,
}

impl Pipes {
    /// Pipes whose randomness is fully determined by `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            pipes: Vec::new(),
            canvas: Vec::new(),
            width: 0,
            height: 0,
            filled: 0,
            pending: 0.0,
        }
    }

    /// How many pipes an area of this size should carry.
    fn pipe_count(area: Rect) -> usize {
        if area.is_empty() {
            return 0;
        }
        let cells = u32::from(area.width) * u32::from(area.height);
        (cells / CELLS_PER_PIPE).clamp(MIN_PIPES, MAX_PIPES) as usize
    }

    /// Wipe the trail and send every pipe off somewhere new.
    fn restart(&mut self) {
        let Self {
            rng,
            pipes,
            canvas,
            width,
            height,
            filled,
            ..
        } = self;

        canvas.fill(None);
        *filled = 0;
        for pipe in pipes.iter_mut() {
            *pipe = Pipe::spawn(rng, *width, *height);
        }
    }

    /// Advance every pipe by one cell.
    fn step(&mut self) {
        let Self {
            rng,
            pipes,
            canvas,
            width,
            height,
            filled,
            ..
        } = self;

        for pipe in pipes.iter_mut() {
            // Decide which way out of this cell, never straight back.
            let outgoing = if rng.random_range(0..100) < TURN_PERCENT {
                if rng.random_range(0..2) == 0 {
                    pipe.dir.turn_left()
                } else {
                    pipe.dir.turn_right()
                }
            } else {
                pipe.dir
            };

            let index = usize::from(pipe.y) * usize::from(*width) + usize::from(pipe.x);
            if canvas[index].is_none() {
                *filled += 1;
            }
            canvas[index] = Some((glyph_for(pipe.dir, outgoing), pipe.color));

            let (dx, dy) = outgoing.delta();
            let next_x = i32::from(pipe.x) + dx;
            let next_y = i32::from(pipe.y) + dy;

            if (0..i32::from(*width)).contains(&next_x) && (0..i32::from(*height)).contains(&next_y)
            {
                pipe.x = next_x as u16;
                pipe.y = next_y as u16;
                pipe.dir = outgoing;
            } else {
                // Off the edge: this pipe's run is over, start another.
                *pipe = Pipe::spawn(rng, *width, *height);
            }
        }

        if self.filled * 100 >= self.canvas.len() * FILL_PERCENT {
            self.restart();
        }
    }
}

impl Animation for Pipes {
    fn name(&self) -> &'static str {
        "pipes"
    }

    fn resize(&mut self, area: Rect) {
        self.width = area.width;
        self.height = area.height;
        self.canvas.clear();
        self.canvas
            .resize(usize::from(area.width) * usize::from(area.height), None);
        self.filled = 0;
        self.pending = 0.0;

        let count = Self::pipe_count(area);
        self.pipes.clear();
        let Self {
            rng,
            pipes,
            width,
            height,
            ..
        } = self;
        pipes.resize_with(count, || Pipe::spawn(rng, *width, *height));
    }

    fn update(&mut self, dt: Duration) {
        if self.canvas.is_empty() || self.pipes.is_empty() {
            return;
        }

        self.pending += dt.as_secs_f32().min(MAX_STEP);
        let step_time = 1.0 / STEPS_PER_SEC;
        while self.pending >= step_time {
            self.pending -= step_time;
            self.step();
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || self.canvas.is_empty() {
            return;
        }

        for y in 0..self.height.min(area.height) {
            for x in 0..self.width.min(area.width) {
                let Some((glyph, color)) =
                    self.canvas[usize::from(y) * usize::from(self.width) + usize::from(x)]
                else {
                    continue;
                };
                if let Some(cell) = buf.cell_mut((area.x + x, area.y + y)) {
                    cell.set_char(glyph).set_fg(rgb(color));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(seed: u64, area: Rect, frames: u32) -> Buffer {
        let mut pipes = Pipes::new(seed);
        pipes.resize(area);
        for _ in 0..frames {
            pipes.update(Duration::from_millis(33));
        }
        let mut buf = Buffer::empty(area);
        pipes.render(area, &mut buf);
        buf
    }

    #[test]
    fn turning_twice_the_same_way_is_a_half_turn() {
        for dir in Dir::ALL {
            assert_eq!(dir.turn_left().turn_left(), dir.opposite());
            assert_eq!(dir.turn_right().turn_right(), dir.opposite());
            assert_eq!(dir.turn_left().turn_right(), dir);
            assert_eq!(dir.opposite().opposite(), dir);
        }
    }

    #[test]
    fn opposite_directions_cancel_out() {
        for dir in Dir::ALL {
            let (dx, dy) = dir.delta();
            let (ox, oy) = dir.opposite().delta();
            assert_eq!((dx + ox, dy + oy), (0, 0));
        }
    }

    #[test]
    fn straight_runs_use_straight_glyphs() {
        assert_eq!(glyph_for(Dir::Up, Dir::Up), '│');
        assert_eq!(glyph_for(Dir::Down, Dir::Down), '│');
        assert_eq!(glyph_for(Dir::Left, Dir::Left), '─');
        assert_eq!(glyph_for(Dir::Right, Dir::Right), '─');
    }

    #[test]
    fn corners_connect_the_right_two_sides() {
        // Travelling right, then turning upwards: the pipe came in from the
        // left and leaves at the top, so the corner opens up and left.
        assert_eq!(glyph_for(Dir::Right, Dir::Up), '┘');
        assert_eq!(glyph_for(Dir::Right, Dir::Down), '┐');
        assert_eq!(glyph_for(Dir::Left, Dir::Up), '└');
        assert_eq!(glyph_for(Dir::Left, Dir::Down), '┌');
        assert_eq!(glyph_for(Dir::Up, Dir::Left), '┐');
        assert_eq!(glyph_for(Dir::Up, Dir::Right), '┌');
        assert_eq!(glyph_for(Dir::Down, Dir::Left), '┘');
        assert_eq!(glyph_for(Dir::Down, Dir::Right), '└');
    }

    #[test]
    fn every_turn_is_drawable() {
        for incoming in Dir::ALL {
            for outgoing in [incoming, incoming.turn_left(), incoming.turn_right()] {
                assert_ne!(
                    glyph_for(incoming, outgoing),
                    '┼',
                    "{incoming:?} then {outgoing:?} fell through to the fallback"
                );
            }
        }
    }

    #[test]
    fn deterministic_with_same_seed() {
        let area = Rect::new(0, 0, 40, 12);
        assert_eq!(advance(42, area, 60), advance(42, area, 60));
    }

    #[test]
    fn different_seeds_give_different_layouts() {
        let area = Rect::new(0, 0, 60, 20);
        assert_ne!(advance(1, area, 60), advance(2, area, 60));
    }

    #[test]
    fn the_trail_builds_up_over_time() {
        let area = Rect::new(0, 0, 60, 20);
        let mut pipes = Pipes::new(4);
        pipes.resize(area);
        for _ in 0..30 {
            pipes.update(Duration::from_millis(33));
        }
        let early = pipes.filled;
        for _ in 0..60 {
            pipes.update(Duration::from_millis(33));
        }
        assert!(pipes.filled > early, "{} vs {early}", pipes.filled);
    }

    #[test]
    fn filled_matches_the_canvas() {
        let area = Rect::new(0, 0, 40, 12);
        let mut pipes = Pipes::new(6);
        pipes.resize(area);
        for _ in 0..120 {
            pipes.update(Duration::from_millis(33));
        }
        let actual = pipes.canvas.iter().filter(|c| c.is_some()).count();
        assert_eq!(pipes.filled, actual, "the coverage counter drifted");
    }

    #[test]
    fn a_full_screen_is_wiped_and_restarted() {
        let area = Rect::new(0, 0, 20, 8);
        let mut pipes = Pipes::new(6);
        pipes.resize(area);

        let mut wiped = false;
        let mut previous = 0;
        for _ in 0..3000 {
            pipes.update(Duration::from_millis(33));
            // Coverage only ever drops when the canvas is wiped.
            wiped |= pipes.filled < previous;
            previous = pipes.filled;
            assert!(
                pipes.filled * 100 < pipes.canvas.len() * (FILL_PERCENT + 5),
                "coverage ran past the limit: {}",
                pipes.filled
            );
        }
        assert!(wiped, "the canvas was never wiped");
    }

    #[test]
    fn stays_inside_the_area() {
        let area = Rect::new(3, 2, 20, 8);
        let mut pipes = Pipes::new(9);
        pipes.resize(area);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 16));

        for _ in 0..300 {
            pipes.update(Duration::from_millis(33));
            pipes.render(area, &mut buf);
        }

        for y in 0..16u16 {
            for x in 0..40u16 {
                if !area.contains((x, y).into()) {
                    assert_eq!(buf[(x, y)].symbol(), " ", "drew outside at {x},{y}");
                }
            }
        }
    }

    #[test]
    fn survives_tiny_and_zero_areas() {
        for area in [
            Rect::new(0, 0, 0, 0),
            Rect::new(0, 0, 1, 1),
            Rect::new(0, 0, 3, 2),
        ] {
            let mut pipes = Pipes::new(1);
            pipes.resize(area);
            pipes.update(Duration::from_millis(33));
            let mut buf = Buffer::empty(area);
            pipes.render(area, &mut buf);
        }
    }

    #[test]
    fn handles_shrinking_after_a_long_run() {
        let mut pipes = Pipes::new(1);
        pipes.resize(Rect::new(0, 0, 200, 60));
        for _ in 0..60 {
            pipes.update(Duration::from_millis(33));
        }

        let small = Rect::new(0, 0, 20, 6);
        pipes.resize(small);
        assert_eq!(pipes.canvas.len(), 20 * 6);
        assert_eq!(pipes.filled, 0);
        let mut buf = Buffer::empty(small);
        pipes.render(small, &mut buf);
    }
}
