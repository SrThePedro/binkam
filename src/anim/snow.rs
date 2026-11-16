//! Drifting snow: flakes fall at their own pace and sway sideways.

use std::time::Duration;

use rand::{RngExt, SeedableRng, rngs::StdRng};
use ratatui::{buffer::Buffer, layout::Rect};

use super::Animation;
use crate::render::{Rgb, lerp};

/// Flake shapes, from the smallest and most distant to the nearest.
const GLYPHS: [char; 4] = ['·', '.', '*', '•'];

/// Falling speed bounds, in rows per second.
const MIN_FALL: f32 = 1.5;
const MAX_FALL: f32 = 7.0;

/// How far a flake drifts from its column, in columns.
const SWAY_WIDTH: f32 = 1.8;

/// Sway speed bounds, in turns per second.
const MIN_SWAY: f32 = 0.08;
const MAX_SWAY: f32 = 0.4;

/// One flake per this many cells.
const CELLS_PER_FLAKE: u32 = 24;

/// Flake count bounds.
const MIN_FLAKES: u32 = 4;
const MAX_FLAKES: u32 = 900;

/// Longest frame gap the animation reacts to.
const MAX_STEP: f32 = 0.1;

/// Colour of the slowest, most distant flakes.
const FAR_COLOR: Rgb = (118, 140, 180);
/// Colour of the nearest flakes.
const NEAR_COLOR: Rgb = (255, 255, 255);

/// A single snowflake.
#[derive(Debug, Clone, Copy)]
struct Flake {
    /// Column it falls along, before the sway is added.
    x: f32,
    /// Row, growing downwards; starts just above the top edge.
    y: f32,
    /// Rows per second.
    fall: f32,
    /// Current sway phase in turns.
    phase: f32,
    /// Turns per second of sway.
    sway: f32,
}

impl Flake {
    /// A flake somewhere in the air, used when first filling the sky.
    fn scattered(rng: &mut StdRng, area: Rect) -> Self {
        Self {
            y: rng.random_range(0.0..f32::from(area.height).max(1.0)),
            ..Self::above(rng, area)
        }
    }

    /// A flake just above the top edge, used when recycling one that landed.
    fn above(rng: &mut StdRng, area: Rect) -> Self {
        Self {
            x: rng.random_range(0.0..f32::from(area.width).max(1.0)),
            y: -rng.random_range(0.0..2.0),
            fall: rng.random_range(MIN_FALL..MAX_FALL),
            phase: rng.random_range(0.0..1.0),
            sway: rng.random_range(MIN_SWAY..MAX_SWAY),
        }
    }

    /// How near this flake reads as, from 0.0 (far) to 1.0 (near).
    fn nearness(&self) -> f32 {
        ((self.fall - MIN_FALL) / (MAX_FALL - MIN_FALL)).clamp(0.0, 1.0)
    }
}

/// The snow animation.
#[derive(Debug)]
pub struct Snow {
    rng: StdRng,
    flakes: Vec<Flake>,
    area: Rect,
}

impl Snow {
    /// Snow whose randomness is fully determined by `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            flakes: Vec::new(),
            area: Rect::ZERO,
        }
    }

    /// How many flakes an area of this size should hold.
    fn flake_count(area: Rect) -> usize {
        if area.is_empty() {
            return 0;
        }
        let cells = u32::from(area.width) * u32::from(area.height);
        (cells / CELLS_PER_FLAKE).clamp(MIN_FLAKES, MAX_FLAKES) as usize
    }
}

impl Animation for Snow {
    fn name(&self) -> &'static str {
        "snow"
    }

    fn resize(&mut self, area: Rect) {
        let Self { rng, flakes, .. } = self;
        // Start over: flake coordinates are in cells, so they do not survive
        // a resize the way the starfield's normalised ones do.
        flakes.clear();
        flakes.resize_with(Self::flake_count(area), || Flake::scattered(rng, area));
        self.area = area;
    }

    fn update(&mut self, dt: Duration) {
        let dt = dt.as_secs_f32().min(MAX_STEP);
        let Self { rng, flakes, area } = self;
        if area.is_empty() {
            return;
        }

        for flake in flakes.iter_mut() {
            flake.y += flake.fall * dt;
            flake.phase += flake.sway * dt;
            if flake.y >= f32::from(area.height) {
                *flake = Flake::above(rng, *area);
            }
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let width = f32::from(area.width);

        for flake in &self.flakes {
            if flake.y < 0.0 {
                continue; // still above the top edge
            }
            let row = flake.y as u16;
            if row >= area.height {
                continue;
            }

            // Wrap sideways so a swaying flake slips round the edge instead
            // of pressing itself against it.
            let drift = (flake.phase * std::f32::consts::TAU).sin() * SWAY_WIDTH;
            let column = (flake.x + drift).rem_euclid(width) as u16;
            if column >= area.width {
                continue;
            }

            let nearness = flake.nearness();
            let glyph = GLYPHS[((nearness * GLYPHS.len() as f32) as usize).min(GLYPHS.len() - 1)];

            if let Some(cell) = buf.cell_mut((area.x + column, area.y + row)) {
                cell.set_char(glyph)
                    .set_fg(lerp(FAR_COLOR, NEAR_COLOR, nearness));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(seed: u64, area: Rect, frames: u32) -> Buffer {
        let mut snow = Snow::new(seed);
        snow.resize(area);
        for _ in 0..frames {
            snow.update(Duration::from_millis(33));
        }
        let mut buf = Buffer::empty(area);
        snow.render(area, &mut buf);
        buf
    }

    #[test]
    fn deterministic_with_same_seed() {
        let area = Rect::new(0, 0, 40, 12);
        assert_eq!(advance(42, area, 30), advance(42, area, 30));
    }

    #[test]
    fn different_seeds_give_different_snow() {
        let area = Rect::new(0, 0, 60, 20);
        assert_ne!(advance(1, area, 20), advance(2, area, 20));
    }

    #[test]
    fn draws_something_on_a_normal_terminal() {
        let area = Rect::new(0, 0, 80, 24);
        let buf = advance(7, area, 60);
        let drawn = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
            .count();
        assert!(drawn > 20, "only {drawn} flakes drawn");
    }

    #[test]
    fn flakes_fall_downwards() {
        let area = Rect::new(0, 0, 40, 40);
        let mut snow = Snow::new(3);
        snow.resize(area);
        let before: Vec<f32> = snow.flakes.iter().map(|f| f.y).collect();
        snow.update(Duration::from_millis(100));
        for (flake, was) in snow.flakes.iter().zip(before) {
            // Either it moved down, or it landed and was recycled to the top.
            assert!(flake.y > was || flake.y < 0.0, "{} vs {was}", flake.y);
        }
    }

    #[test]
    fn landed_flakes_are_recycled() {
        let area = Rect::new(0, 0, 30, 10);
        let mut snow = Snow::new(2);
        snow.resize(area);
        for _ in 0..600 {
            snow.update(Duration::from_millis(33));
        }
        assert!(
            snow.flakes.iter().all(|f| f.y < f32::from(area.height)),
            "a flake fell through the floor"
        );
    }

    #[test]
    fn stays_inside_the_area() {
        let area = Rect::new(4, 2, 20, 8);
        let mut snow = Snow::new(9);
        snow.resize(area);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 16));

        for _ in 0..200 {
            snow.update(Duration::from_millis(33));
            snow.render(area, &mut buf);
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
    fn population_scales_with_the_area() {
        let mut snow = Snow::new(3);
        snow.resize(Rect::new(0, 0, 200, 60));
        let big = snow.flakes.len();
        snow.resize(Rect::new(0, 0, 20, 6));
        let small = snow.flakes.len();
        assert!(big > small, "{big} should beat {small}");
    }

    #[test]
    fn survives_tiny_and_zero_areas() {
        for area in [
            Rect::new(0, 0, 0, 0),
            Rect::new(0, 0, 1, 1),
            Rect::new(0, 0, 3, 2),
        ] {
            let mut snow = Snow::new(1);
            snow.resize(area);
            snow.update(Duration::from_millis(33));
            let mut buf = Buffer::empty(area);
            snow.render(area, &mut buf);
        }
    }

    #[test]
    fn handles_shrinking_after_a_long_run() {
        let mut snow = Snow::new(1);
        snow.resize(Rect::new(0, 0, 200, 60));
        snow.update(Duration::from_millis(500));

        let small = Rect::new(0, 0, 20, 6);
        snow.resize(small);
        let mut buf = Buffer::empty(small);
        snow.render(small, &mut buf);
    }
}
