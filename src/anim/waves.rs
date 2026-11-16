//! Overlapping sine waves, drawn as thin lines on a half-block grid.
//!
//! Each line is a horizontal baseline displaced by two sine waves of
//! different wavelengths, which keeps the motion from looking periodic. The
//! lines are computed into a pixel buffer during `update` and only blitted
//! in `render`, so drawing stays a straight copy.

use std::{f32::consts::TAU, time::Duration};

use ratatui::{buffer::Buffer, layout::Rect};

use super::Animation;
use crate::render::{Rgb, lerp_rgb, put_pixel_pair_opt};

/// Pixel rows between two baselines.
const LINE_SPACING: u16 = 5;

/// Wavelengths of the two waves, in pixels.
const WAVE_LONG: f32 = 31.0;
const WAVE_SHORT: f32 = 17.0;

/// Amplitudes of the two waves, in pixels.
const AMPLITUDE_LONG: f32 = 3.6;
const AMPLITUDE_SHORT: f32 = 1.7;

/// Phase added per second by each wave, in turns.
const SPEED_LONG: f32 = 0.11;
const SPEED_SHORT: f32 = -0.19;

/// Phase wraps here. Both speeds are hundredths, so a hundred turns is a
/// period they share and the pattern stays continuous across the wrap.
const TIME_PERIOD: f32 = 100.0;

/// Phase offset between neighbouring lines, in turns, so they do not move as
/// one block.
const LINE_OFFSET: f32 = 0.085;

/// Longest frame gap the animation reacts to.
const MAX_STEP: f32 = 0.1;

/// Colour of the nearest line, at the bottom of the screen.
const NEAR_COLOR: Rgb = (18, 48, 108);
/// Colour of the furthest line, at the top.
const FAR_COLOR: Rgb = (96, 214, 226);

/// The waves animation.
#[derive(Debug)]
pub struct Waves {
    /// Colour per pixel, `None` where no line passes, `width * pixel_height`.
    pixels: Vec<Option<Rgb>>,
    width: u16,
    pixel_height: u16,
    /// Phase in turns, kept inside `0.0..TIME_PERIOD`.
    time: f32,
}

impl Waves {
    /// Waves whose starting phase is derived from `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            pixels: Vec::new(),
            width: 0,
            pixel_height: 0,
            time: (seed % 1000) as f32 * TIME_PERIOD / 1000.0,
        }
    }

    /// How many baselines fit on the current grid.
    fn line_count(&self) -> u16 {
        self.pixel_height / LINE_SPACING
    }

    /// Redraw every line into [`Waves::pixels`].
    fn plot(&mut self) {
        self.pixels.fill(None);

        let lines = self.line_count();
        if lines == 0 || self.width == 0 {
            return;
        }
        let stride = usize::from(self.width);

        for line in 0..lines {
            // Line 0 sits at the bottom and is the nearest.
            let baseline = f32::from(self.pixel_height - 1 - line * LINE_SPACING);
            let depth = f32::from(line) / f32::from(lines.max(1));
            let color = lerp_rgb(NEAR_COLOR, FAR_COLOR, depth);
            let phase = f32::from(line) * LINE_OFFSET;

            for x in 0..self.width {
                let fx = f32::from(x);
                let long = ((fx / WAVE_LONG + self.time * SPEED_LONG + phase) * TAU).sin();
                let short = ((fx / WAVE_SHORT + self.time * SPEED_SHORT - phase) * TAU).sin();
                let y = baseline + long * AMPLITUDE_LONG + short * AMPLITUDE_SHORT;

                if y < 0.0 {
                    continue;
                }
                let row = y as u16;
                if row >= self.pixel_height {
                    continue;
                }
                self.pixels[usize::from(row) * stride + usize::from(x)] = Some(color);
            }
        }
    }
}

impl Animation for Waves {
    fn name(&self) -> &'static str {
        "waves"
    }

    fn resize(&mut self, area: Rect) {
        self.width = area.width;
        self.pixel_height = area.height.saturating_mul(2);
        self.pixels.clear();
        self.pixels.resize(
            usize::from(self.width) * usize::from(self.pixel_height),
            None,
        );
        self.plot();
    }

    fn update(&mut self, dt: Duration) {
        if self.pixels.is_empty() {
            return;
        }
        self.time = (self.time + dt.as_secs_f32().min(MAX_STEP)).rem_euclid(TIME_PERIOD);
        self.plot();
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || self.pixels.is_empty() {
            return;
        }

        let stride = usize::from(self.width);
        let rows = self.pixel_height / 2;
        for y in 0..rows.min(area.height) {
            for x in 0..self.width.min(area.width) {
                let top = self.pixels[usize::from(y * 2) * stride + usize::from(x)];
                let bottom = self.pixels[usize::from(y * 2 + 1) * stride + usize::from(x)];
                put_pixel_pair_opt(buf, area.x + x, area.y + y, top, bottom);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(seed: u64, area: Rect, frames: u32) -> Buffer {
        let mut waves = Waves::new(seed);
        waves.resize(area);
        for _ in 0..frames {
            waves.update(Duration::from_millis(33));
        }
        let mut buf = Buffer::empty(area);
        waves.render(area, &mut buf);
        buf
    }

    #[test]
    fn deterministic_with_same_seed() {
        let area = Rect::new(0, 0, 40, 12);
        assert_eq!(advance(42, area, 30), advance(42, area, 30));
    }

    #[test]
    fn different_seeds_start_at_different_phases() {
        let area = Rect::new(0, 0, 40, 12);
        assert_ne!(advance(1, area, 5), advance(600, area, 5));
    }

    #[test]
    fn the_pattern_moves_over_time() {
        let area = Rect::new(0, 0, 40, 12);
        assert_ne!(advance(1, area, 1), advance(1, area, 90));
    }

    #[test]
    fn phase_stays_bounded_forever() {
        let mut waves = Waves::new(0);
        waves.resize(Rect::new(0, 0, 20, 6));
        for _ in 0..20_000 {
            waves.update(Duration::from_millis(33));
        }
        assert!((0.0..TIME_PERIOD).contains(&waves.time), "{}", waves.time);
    }

    #[test]
    fn every_line_draws_one_pixel_per_column() {
        let area = Rect::new(0, 0, 40, 12);
        let mut waves = Waves::new(2);
        waves.resize(area);

        // One pixel per column per line, minus any that fell off the top.
        let lit = waves.pixels.iter().filter(|p| p.is_some()).count();
        let expected = usize::from(area.width) * usize::from(waves.line_count());
        assert!(lit <= expected, "{lit} lit, at most {expected} expected");
        assert!(lit > expected / 2, "only {lit} of {expected} lit");
    }

    #[test]
    fn lines_are_coloured_by_depth() {
        let area = Rect::new(0, 0, 40, 24);
        let mut waves = Waves::new(2);
        waves.resize(area);

        let colors: Vec<Rgb> = waves.pixels.iter().flatten().copied().collect();
        assert!(colors.contains(&NEAR_COLOR), "the bottom line is missing");
        assert!(
            colors.iter().any(|c| *c != NEAR_COLOR),
            "every line came out the same colour"
        );
    }

    #[test]
    fn draws_something_on_a_normal_terminal() {
        let area = Rect::new(0, 0, 80, 24);
        let buf = advance(7, area, 30);
        let drawn = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
            .count();
        assert!(drawn > 200, "only {drawn} cells drawn");
    }

    #[test]
    fn stays_inside_the_area() {
        let area = Rect::new(3, 2, 20, 8);
        let mut waves = Waves::new(9);
        waves.resize(area);
        waves.update(Duration::from_millis(33));
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 16));
        waves.render(area, &mut buf);

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
            let mut waves = Waves::new(1);
            waves.resize(area);
            waves.update(Duration::from_millis(33));
            let mut buf = Buffer::empty(area);
            waves.render(area, &mut buf);
        }
    }

    #[test]
    fn handles_shrinking_after_a_long_run() {
        let mut waves = Waves::new(1);
        waves.resize(Rect::new(0, 0, 200, 60));
        waves.update(Duration::from_millis(500));

        let small = Rect::new(0, 0, 20, 6);
        waves.resize(small);
        assert_eq!(waves.pixels.len(), 20 * 12);
        let mut buf = Buffer::empty(small);
        waves.render(small, &mut buf);
    }
}
