//! DOOM style fire, drawn with half-block pixels.
//!
//! The classic trick: keep a heat field, keep its bottom row burning, and
//! every step let each pixel take the heat of the pixel below it, minus a
//! little random cooling and shifted sideways by at most one column. Rising,
//! flickering flames fall out of those three rules on their own.

use std::time::Duration;

use rand::{RngExt, SeedableRng, rngs::StdRng};
use ratatui::{buffer::Buffer, layout::Rect};

use super::Animation;
use crate::render::{Rgb, lerp_rgb, put_pixel_pair, rgb};

/// Simulation steps per second, kept fixed so the flames look the same at
/// any frame rate.
const STEPS_PER_SEC: f32 = 30.0;

/// Longest frame gap the simulation reacts to.
///
/// This is also what bounds the work per frame: at most
/// `MAX_STEP * STEPS_PER_SEC` steps ever run for one frame, so waking from
/// sleep can never turn into a burst of catch-up work.
const MAX_STEP: f32 = 0.1;

/// Share of bottom-row pixels that burn at full heat; the rest are embers.
const EMBER_CHANCE: u32 = 8;

/// Fraction of the pixel height the flames should burn out within.
const FLAME_REACH: f32 = 0.6;

/// Cooling is clamped to this range whatever the terminal size.
const COOLING_RANGE: (u8, u8) = (2, 96);

/// Colour stops the 256 entry palette is interpolated from.
const STOPS: [(f32, Rgb); 5] = [
    (0.00, (0, 0, 0)),
    (0.22, (120, 12, 0)),
    (0.48, (235, 90, 0)),
    (0.78, (255, 205, 60)),
    (1.00, (255, 255, 225)),
];

/// The fire animation.
#[derive(Debug)]
pub struct Fire {
    rng: StdRng,
    /// Heat per pixel, row-major, `width * pixel_height` long.
    heat: Vec<u8>,
    /// Cells across.
    width: u16,
    /// Pixel rows: twice the cell rows, thanks to half blocks.
    pixel_height: u16,
    /// Heat to colour lookup, built once so drawing is pure table reads.
    palette: [Rgb; 256],
    /// Most heat a pixel may lose in one step, scaled to the screen height so
    /// the flames reach about the same fraction of any terminal.
    cooling: u8,
    /// Simulation time not yet consumed by a step.
    pending: f32,
}

impl Fire {
    /// A fire whose randomness is fully determined by `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            heat: Vec::new(),
            width: 0,
            pixel_height: 0,
            palette: build_palette(),
            cooling: COOLING_RANGE.1,
            pending: 0.0,
        }
    }

    /// Index of a pixel in [`Fire::heat`].
    fn index(&self, x: u16, y: u16) -> usize {
        usize::from(y) * usize::from(self.width) + usize::from(x)
    }

    /// One step of the heat simulation.
    fn step(&mut self) {
        let Self {
            rng,
            heat,
            width,
            pixel_height,
            cooling,
            ..
        } = self;

        let w = i32::from(*width);
        let stride = usize::from(*width);
        let row_start = usize::from(*pixel_height - 1) * stride;

        // Keep the bottom row alight, with the odd cooler ember for texture.
        for x in 0..stride {
            heat[row_start + x] = if rng.random_range(0..100) < EMBER_CHANCE {
                rng.random_range(90..190)
            } else {
                u8::MAX
            };
        }

        // Walk the destinations, not the sources: every pixel above the
        // bottom row is then written exactly once per step, so none keeps a
        // stale value and turns into a hot spot that never fades.
        for y in 1..*pixel_height {
            for x in 0..*width {
                let drift = rng.random_range(-1..=1);
                // Wrap sideways rather than clamp, so heat does not pile up
                // against the left and right edges.
                let source_x = (i32::from(x) + drift).rem_euclid(w) as usize;
                let source = heat[usize::from(y) * stride + source_x];
                let loss = rng.random_range(0..=*cooling);
                heat[usize::from(y - 1) * stride + usize::from(x)] = source.saturating_sub(loss);
            }
        }
    }
}

impl Animation for Fire {
    fn name(&self) -> &'static str {
        "fire"
    }

    fn resize(&mut self, area: Rect) {
        self.width = area.width;
        self.pixel_height = area.height.saturating_mul(2);
        self.heat.clear();
        self.heat
            .resize(usize::from(self.width) * usize::from(self.pixel_height), 0);
        self.cooling = cooling_for(self.pixel_height);
        self.pending = 0.0;
    }

    fn update(&mut self, dt: Duration) {
        if self.heat.is_empty() {
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
        if area.is_empty() || self.heat.is_empty() {
            return;
        }

        let rows = self.pixel_height / 2;
        for y in 0..rows.min(area.height) {
            for x in 0..self.width.min(area.width) {
                let top = self.heat[self.index(x, y * 2)];
                let bottom = self.heat[self.index(x, y * 2 + 1)];
                if top == 0 && bottom == 0 {
                    continue; // cold pixel pair: leave the background alone
                }
                put_pixel_pair(
                    buf,
                    area.x + x,
                    area.y + y,
                    rgb(self.palette[usize::from(top)]),
                    rgb(self.palette[usize::from(bottom)]),
                );
            }
        }
    }
}

/// Cooling per step that burns full heat out over [`FLAME_REACH`] of the
/// screen.
///
/// A loss of `0..=cooling` averages `cooling / 2`, so reaching zero after
/// `rows` rows needs `cooling = 2 * 255 / rows`.
fn cooling_for(pixel_height: u16) -> u8 {
    let rows = f32::from(pixel_height) * FLAME_REACH;
    if rows < 1.0 {
        return COOLING_RANGE.1;
    }
    let (low, high) = COOLING_RANGE;
    ((2.0 * 255.0 / rows).round() as u32).clamp(u32::from(low), u32::from(high)) as u8
}

/// Interpolate [`STOPS`] into a 256 entry heat ramp.
fn build_palette() -> [Rgb; 256] {
    let mut palette = [(0, 0, 0); 256];
    for (level, slot) in palette.iter_mut().enumerate() {
        let t = level as f32 / 255.0;

        // Find the pair of stops this level falls between.
        let mut first = 0;
        while first + 2 < STOPS.len() && t > STOPS[first + 1].0 {
            first += 1;
        }
        let (from_t, from_color) = STOPS[first];
        let (to_t, to_color) = STOPS[first + 1];

        let local = if to_t > from_t {
            (t - from_t) / (to_t - from_t)
        } else {
            0.0
        };
        *slot = lerp_rgb(from_color, to_color, local);
    }
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(seed: u64, area: Rect, frames: u32) -> Buffer {
        let mut fire = Fire::new(seed);
        fire.resize(area);
        for _ in 0..frames {
            fire.update(Duration::from_millis(33));
        }
        let mut buf = Buffer::empty(area);
        fire.render(area, &mut buf);
        buf
    }

    #[test]
    fn palette_runs_from_black_to_near_white() {
        let palette = build_palette();
        assert_eq!(palette[0], (0, 0, 0));
        assert_eq!(palette[255], (255, 255, 225));
        // Brightness has to rise across the whole ramp.
        let luma = |c: Rgb| u32::from(c.0) + u32::from(c.1) + u32::from(c.2);
        for level in 1..256 {
            assert!(
                luma(palette[level]) >= luma(palette[level - 1]),
                "palette dips at {level}"
            );
        }
    }

    #[test]
    fn cooling_scales_with_the_screen_height() {
        assert!(
            cooling_for(200) < cooling_for(48),
            "taller screens cool less"
        );
        assert_eq!(cooling_for(0), COOLING_RANGE.1);
        assert!((COOLING_RANGE.0..=COOLING_RANGE.1).contains(&cooling_for(4000)));
    }

    #[test]
    fn flames_burn_out_before_the_top_of_a_tall_screen() {
        // Regression: with a fixed cooling rate the heat reached the very top
        // row and the fire covered the whole screen.
        let area = Rect::new(0, 0, 80, 24);
        let mut fire = Fire::new(5);
        fire.resize(area);
        for _ in 0..150 {
            fire.update(Duration::from_millis(33));
        }

        let row_heat = |y: u16| -> u32 {
            (0..area.width)
                .map(|x| u32::from(fire.heat[fire.index(x, y)]))
                .sum()
        };

        for y in 0..fire.pixel_height / 5 {
            assert_eq!(row_heat(y), 0, "pixel row {y} is still hot");
        }
        assert!(row_heat(fire.pixel_height - 1) > 0, "bottom row went out");
        assert!(
            row_heat(fire.pixel_height / 2) > 0,
            "the flames should still reach halfway"
        );
    }

    #[test]
    fn deterministic_with_same_seed() {
        let area = Rect::new(0, 0, 40, 12);
        assert_eq!(advance(42, area, 30), advance(42, area, 30));
    }

    #[test]
    fn different_seeds_give_different_flames() {
        let area = Rect::new(0, 0, 60, 20);
        assert_ne!(advance(1, area, 30), advance(2, area, 30));
    }

    #[test]
    fn heat_rises_and_cools() {
        let area = Rect::new(0, 0, 40, 12);
        let mut fire = Fire::new(3);
        fire.resize(area);
        for _ in 0..120 {
            fire.update(Duration::from_millis(33));
        }

        let row_heat = |y: u16| -> u32 {
            (0..area.width)
                .map(|x| u32::from(fire.heat[fire.index(x, y)]))
                .sum()
        };
        let bottom = row_heat(fire.pixel_height - 1);
        let middle = row_heat(fire.pixel_height / 2);
        let top = row_heat(0);

        assert!(bottom > middle, "{bottom} should beat {middle}");
        assert!(middle > top, "{middle} should beat {top}");
    }

    #[test]
    fn draws_half_blocks_along_the_bottom() {
        let area = Rect::new(0, 0, 40, 12);
        let buf = advance(5, area, 60);
        let bottom_row = (0..area.width)
            .filter(|&x| buf[(x, area.height - 1)].symbol() == "▀")
            .count();
        assert_eq!(usize::from(area.width), bottom_row, "bottom row not alight");
    }

    #[test]
    fn stays_inside_the_area() {
        let area = Rect::new(3, 2, 20, 8);
        let mut fire = Fire::new(9);
        fire.resize(area);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 16));

        for _ in 0..90 {
            fire.update(Duration::from_millis(33));
            fire.render(area, &mut buf);
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
    fn a_long_stall_runs_a_bounded_number_of_steps() {
        let mut fire = Fire::new(1);
        let area = Rect::new(0, 0, 40, 12);
        fire.resize(area);
        fire.update(Duration::from_secs(3600));

        // The dt clamp is the only bound needed: no backlog is carried over.
        assert!(
            fire.pending < 1.0 / STEPS_PER_SEC,
            "pending {} should be under one step",
            fire.pending
        );
        // One clamped frame is enough to light the bottom row, no more.
        assert_eq!(fire.heat[fire.index(0, fire.pixel_height - 1)], u8::MAX);
        assert_eq!(fire.heat[fire.index(0, 0)], 0, "fire should not have risen");
    }

    #[test]
    fn survives_tiny_and_zero_areas() {
        for area in [
            Rect::new(0, 0, 0, 0),
            Rect::new(0, 0, 1, 1),
            Rect::new(0, 0, 3, 2),
        ] {
            let mut fire = Fire::new(1);
            fire.resize(area);
            fire.update(Duration::from_millis(33));
            let mut buf = Buffer::empty(area);
            fire.render(area, &mut buf);
        }
    }

    #[test]
    fn handles_shrinking_after_a_long_run() {
        let mut fire = Fire::new(1);
        fire.resize(Rect::new(0, 0, 200, 60));
        for _ in 0..30 {
            fire.update(Duration::from_millis(33));
        }

        let small = Rect::new(0, 0, 20, 6);
        fire.resize(small);
        assert_eq!(fire.heat.len(), 20 * 12);
        let mut buf = Buffer::empty(small);
        fire.render(small, &mut buf);
    }
}
