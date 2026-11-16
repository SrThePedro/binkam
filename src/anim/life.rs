//! Conway's Game of Life on a half-block pixel grid.
//!
//! The grid wraps around at the edges, so patterns drift off one side and
//! come back on the other instead of dying against a wall. Cells that just
//! died leave a fading trail, which turns a rather static simulation into
//! something worth looking at.

use std::time::Duration;

use rand::{RngExt, SeedableRng, rngs::StdRng};
use ratatui::{buffer::Buffer, layout::Rect};

use super::Animation;
use crate::render::{Rgb, lerp_rgb, put_pixel_pair_opt, wrap_by_one};

/// Generations per second.
const STEPS_PER_SEC: f32 = 9.0;

/// Longest frame gap the simulation reacts to, which also bounds the work
/// done for any single frame.
const MAX_STEP: f32 = 0.1;

/// Share of cells alive in a fresh seeding.
const SEED_DENSITY: f32 = 0.3;

/// Glow lost per generation by a cell that is no longer alive.
const GLOW_DECAY: u8 = 26;

/// Generations of repetition tolerated before the grid is seeded again.
const STAGNANT_LIMIT: u32 = 16;

/// Colour of a living cell.
const ALIVE_COLOR: Rgb = (190, 255, 240);
/// Colour of a cell that died a moment ago.
const TRAIL_HOT: Rgb = (36, 150, 185);
/// Colour the trail fades out to.
const TRAIL_COLD: Rgb = (6, 16, 38);

/// The Game of Life animation.
#[derive(Debug)]
pub struct Life {
    rng: StdRng,
    /// Living cells, row-major, `width * pixel_height` long.
    alive: Vec<bool>,
    /// Scratch buffer for the next generation, swapped in rather than
    /// reallocated every step.
    next: Vec<bool>,
    /// Fading trail left behind by cells that died.
    glow: Vec<u8>,
    width: u16,
    pixel_height: u16,
    /// Simulation time not yet consumed by a generation.
    pending: f32,
    /// Fingerprints of the two previous generations, to spot still lifes and
    /// period-two blinkers.
    history: [u64; 2],
    /// How many generations in a row have repeated an earlier one.
    stagnant: u32,
}

impl Life {
    /// A simulation whose randomness is fully determined by `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            alive: Vec::new(),
            next: Vec::new(),
            glow: Vec::new(),
            width: 0,
            pixel_height: 0,
            pending: 0.0,
            history: [0; 2],
            stagnant: 0,
        }
    }

    /// Index of a pixel in the grids.
    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.width) + usize::from(x)
    }

    /// Fill the grid with random life and forget the trail and the history.
    fn seed_grid(&mut self) {
        let Self {
            rng, alive, glow, ..
        } = self;
        for (cell, trail) in alive.iter_mut().zip(glow.iter_mut()) {
            *cell = rng.random::<f32>() < SEED_DENSITY;
            *trail = if *cell { u8::MAX } else { 0 };
        }
        self.history = [0; 2];
        self.stagnant = 0;
    }

    /// Living neighbours of a cell, counted on a wrapping grid.
    fn neighbours(&self, x: u16, y: u16) -> u8 {
        let w = i32::from(self.width);
        let h = i32::from(self.pixel_height);
        let mut count = 0;

        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                // Neighbours are one step away at most, so the two edge
                // cases are all the wrapping needed. `rem_euclid` would be
                // two integer divisions per neighbour, eight neighbours per
                // cell, for every cell of every generation.
                let nx = wrap_by_one(i32::from(x) + dx, w);
                let ny = wrap_by_one(i32::from(y) + dy, h);
                count += u8::from(self.alive[ny * usize::from(self.width) + nx]);
            }
        }
        count
    }

    /// A cheap fingerprint of the current generation (FNV-1a).
    fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for &cell in &self.alive {
            hash ^= u64::from(cell);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    /// Advance one generation.
    fn step(&mut self) {
        for y in 0..self.pixel_height {
            for x in 0..self.width {
                let index = self.index(x, y);
                let neighbours = self.neighbours(x, y);
                // Conway's B3/S23: born on three, survives on two or three.
                self.next[index] =
                    matches!((self.alive[index], neighbours), (true, 2 | 3) | (false, 3));
            }
        }

        std::mem::swap(&mut self.alive, &mut self.next);

        for (cell, trail) in self.alive.iter().zip(self.glow.iter_mut()) {
            *trail = if *cell {
                u8::MAX
            } else {
                trail.saturating_sub(GLOW_DECAY)
            };
        }

        // A generation that repeats either of the last two is a still life or
        // a blinker; enough of those in a row and it is time to start over.
        let fingerprint = self.fingerprint();
        if self.history.contains(&fingerprint) {
            self.stagnant += 1;
        } else {
            self.stagnant = 0;
        }
        self.history = [self.history[1], fingerprint];

        if self.stagnant >= STAGNANT_LIMIT {
            self.seed_grid();
        }
    }

    /// Colour of one pixel, or `None` when it is fully dark.
    fn color_of(&self, index: usize) -> Option<Rgb> {
        if self.alive[index] {
            return Some(ALIVE_COLOR);
        }
        match self.glow[index] {
            0 => None,
            trail => Some(lerp_rgb(TRAIL_COLD, TRAIL_HOT, f32::from(trail) / 255.0)),
        }
    }
}

impl Animation for Life {
    fn name(&self) -> &'static str {
        "life"
    }

    fn resize(&mut self, area: Rect) {
        self.width = area.width;
        self.pixel_height = area.height.saturating_mul(2);
        let cells = usize::from(self.width) * usize::from(self.pixel_height);

        self.alive.clear();
        self.alive.resize(cells, false);
        self.next.clear();
        self.next.resize(cells, false);
        self.glow.clear();
        self.glow.resize(cells, 0);
        self.pending = 0.0;

        self.seed_grid();
    }

    fn update(&mut self, dt: Duration) {
        if self.alive.is_empty() {
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
        if area.is_empty() || self.alive.is_empty() {
            return;
        }

        let rows = self.pixel_height / 2;
        for y in 0..rows.min(area.height) {
            for x in 0..self.width.min(area.width) {
                put_pixel_pair_opt(
                    buf,
                    area.x + x,
                    area.y + y,
                    self.color_of(self.index(x, y * 2)),
                    self.color_of(self.index(x, y * 2 + 1)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(seed: u64, area: Rect, frames: u32) -> Buffer {
        let mut life = Life::new(seed);
        life.resize(area);
        for _ in 0..frames {
            life.update(Duration::from_millis(33));
        }
        let mut buf = Buffer::empty(area);
        life.render(area, &mut buf);
        buf
    }

    /// Put a single pattern on an otherwise empty grid.
    fn with_pattern(area: Rect, cells: &[(u16, u16)]) -> Life {
        let mut life = Life::new(1);
        life.resize(area);
        life.alive.fill(false);
        life.glow.fill(0);
        for &(x, y) in cells {
            let index = life.index(x, y);
            life.alive[index] = true;
            life.glow[index] = u8::MAX;
        }
        life.history = [0; 2];
        life.stagnant = 0;
        life
    }

    #[test]
    fn a_block_is_a_still_life() {
        let area = Rect::new(0, 0, 10, 5);
        let mut life = with_pattern(area, &[(2, 2), (3, 2), (2, 3), (3, 3)]);
        let before = life.alive.clone();
        life.step();
        assert_eq!(life.alive, before, "a 2x2 block must not change");
    }

    #[test]
    fn a_blinker_has_period_two() {
        let area = Rect::new(0, 0, 10, 5);
        let mut life = with_pattern(area, &[(3, 2), (3, 3), (3, 4)]);
        let before = life.alive.clone();
        life.step();
        assert_ne!(life.alive, before, "a blinker should turn");
        life.step();
        assert_eq!(life.alive, before, "and turn back");
    }

    #[test]
    fn lone_cells_die() {
        let area = Rect::new(0, 0, 10, 5);
        let mut life = with_pattern(area, &[(4, 4)]);
        life.step();
        assert!(
            !life.alive.iter().any(|&c| c),
            "a single cell has no support"
        );
    }

    #[test]
    fn the_grid_wraps_around_its_edges() {
        let area = Rect::new(0, 0, 10, 5);
        // A vertical blinker split across the top and bottom edges.
        let mut life = with_pattern(area, &[(5, 0), (5, 9), (5, 8)]);
        life.step();
        // Wrapping makes this a blinker, so it becomes a horizontal row.
        assert!(
            life.alive[life.index(4, 9)] && life.alive[life.index(6, 9)],
            "neighbours did not wrap across the edge"
        );
    }

    #[test]
    fn a_still_life_is_eventually_reseeded() {
        let area = Rect::new(0, 0, 10, 5);
        let mut life = with_pattern(area, &[(2, 2), (3, 2), (2, 3), (3, 3)]);
        for _ in 0..STAGNANT_LIMIT + 2 {
            life.step();
        }
        let population = life.alive.iter().filter(|&&c| c).count();
        assert!(
            population > 4,
            "the grid should have been reseeded: {population}"
        );
        assert_eq!(life.stagnant, 0, "the counter resets on reseeding");
    }

    #[test]
    fn a_living_grid_is_left_alone() {
        let area = Rect::new(0, 0, 60, 20);
        let mut life = Life::new(4);
        life.resize(area);
        for _ in 0..40 {
            life.step();
        }
        assert!(life.stagnant < STAGNANT_LIMIT);
    }

    #[test]
    fn trails_fade_out_after_a_death() {
        let area = Rect::new(0, 0, 10, 5);
        let mut life = with_pattern(area, &[(4, 4)]);
        life.step(); // the cell dies, its glow starts at full
        let index = life.index(4, 4);
        let first = life.glow[index];
        assert!(first < u8::MAX && first > 0, "glow should start fading");
        for _ in 0..20 {
            life.step();
        }
        assert_eq!(life.glow[index], 0, "glow should reach zero");
    }

    #[test]
    fn deterministic_with_same_seed() {
        let area = Rect::new(0, 0, 40, 12);
        assert_eq!(advance(42, area, 30), advance(42, area, 30));
    }

    #[test]
    fn different_seeds_give_different_grids() {
        let area = Rect::new(0, 0, 60, 20);
        assert_ne!(advance(1, area, 10), advance(2, area, 10));
    }

    #[test]
    fn draws_something_on_a_normal_terminal() {
        let area = Rect::new(0, 0, 80, 24);
        let buf = advance(7, area, 60);
        let drawn = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
            .count();
        assert!(drawn > 100, "only {drawn} cells drawn");
    }

    #[test]
    fn stays_inside_the_area() {
        let area = Rect::new(3, 2, 20, 8);
        let mut life = Life::new(9);
        life.resize(area);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 16));

        for _ in 0..60 {
            life.update(Duration::from_millis(33));
            life.render(area, &mut buf);
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
            let mut life = Life::new(1);
            life.resize(area);
            life.update(Duration::from_millis(33));
            let mut buf = Buffer::empty(area);
            life.render(area, &mut buf);
        }
    }

    #[test]
    fn handles_shrinking_after_a_long_run() {
        let mut life = Life::new(1);
        life.resize(Rect::new(0, 0, 200, 60));
        life.update(Duration::from_millis(500));

        let small = Rect::new(0, 0, 20, 6);
        life.resize(small);
        assert_eq!(life.alive.len(), 20 * 12);
        let mut buf = Buffer::empty(small);
        life.render(small, &mut buf);
    }
}
