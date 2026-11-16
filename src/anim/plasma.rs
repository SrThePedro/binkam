//! Truecolor plasma: a sum of sine waves sampled per pixel.
//!
//! The only state is elapsed time. Every pixel's colour is a pure function of
//! its position and that time, which makes this the cheapest animation to
//! reason about and the easiest to test.

use std::{f32::consts::TAU, time::Duration};

use ratatui::{buffer::Buffer, layout::Rect};

use super::Animation;
use crate::render::{Rgb, lerp_rgb, put_pixel_pair, rgb};

/// Entries in the sine lookup table. A power of two, so wrapping an index is
/// a bit mask rather than a division.
const SIN_STEPS: usize = 1024;

/// Turns of phase added per second.
const TIME_SCALE: f32 = 0.11;

/// Phase wraps here instead of growing without bound and losing precision.
///
/// All four time multipliers below are tenths, so ten turns is a period they
/// share: wrapping there keeps the pattern continuous.
const TIME_PERIOD: f32 = 10.0;

/// Longest frame gap the animation reacts to.
const MAX_STEP: f32 = 0.1;

/// Wavelengths in pixels for the four waves.
const WAVE_X: f32 = 26.0;
const WAVE_Y: f32 = 34.0;
const WAVE_DIAGONAL: f32 = 46.0;
const WAVE_RADIAL: f32 = 21.0;

/// Colour stops of the cyclic palette; the last must equal the first.
const STOPS: [Rgb; 6] = [
    (26, 34, 92),
    (104, 42, 148),
    (206, 64, 118),
    (226, 142, 72),
    (48, 122, 176),
    (26, 34, 92),
];

/// The plasma animation.
#[derive(Debug)]
pub struct Plasma {
    /// One period of a sine, scaled to `-1.0..=1.0`.
    sine: [f32; SIN_STEPS],
    /// Cyclic colour ramp indexed by the normalised wave sum.
    palette: [Rgb; 256],
    /// Phase in turns, kept inside `0.0..TIME_PERIOD`.
    time: f32,
}

impl Plasma {
    /// A plasma whose starting phase is derived from `seed`.
    pub fn new(seed: u64) -> Self {
        let mut sine = [0.0; SIN_STEPS];
        for (step, value) in sine.iter_mut().enumerate() {
            *value = (TAU * step as f32 / SIN_STEPS as f32).sin();
        }

        Self {
            sine,
            palette: build_palette(),
            // Different seeds start the pattern at a different phase.
            time: (seed % 1000) as f32 * TIME_PERIOD / 1000.0,
        }
    }

    /// Sine of `turns`, read from the table instead of computed.
    fn sine_at(&self, turns: f32) -> f32 {
        // `as usize` on a non-negative float truncates, and the mask keeps the
        // index in range even when rounding lands exactly on SIN_STEPS.
        let step = (turns.rem_euclid(1.0) * SIN_STEPS as f32) as usize;
        self.sine[step & (SIN_STEPS - 1)]
    }

    /// Colour of one pixel in a `width` by `pixel_height` grid.
    ///
    /// Half-block pixels are already square on screen, so no aspect
    /// correction is needed here.
    fn pixel(&self, x: u16, y: u16, center_x: f32, center_y: f32) -> Rgb {
        let fx = f32::from(x);
        let fy = f32::from(y);
        let dx = fx - center_x;
        let dy = fy - center_y;
        let radius = dx.hypot(dy);
        let t = self.time;

        let sum = self.sine_at(fx / WAVE_X + t)
            + self.sine_at(fy / WAVE_Y + t * 1.3)
            + self.sine_at((fx + fy) / WAVE_DIAGONAL + t * 0.7)
            + self.sine_at(radius / WAVE_RADIAL - t * 1.1);

        // Four waves give -4.0..=4.0; fold that onto the palette.
        let level = ((sum + 4.0) / 8.0 * 255.0).clamp(0.0, 255.0) as usize;
        self.palette[level]
    }
}

impl Animation for Plasma {
    fn name(&self) -> &'static str {
        "plasma"
    }

    fn resize(&mut self, _area: Rect) {
        // Nothing to do: the pattern is a function of position and time.
    }

    fn update(&mut self, dt: Duration) {
        self.time += dt.as_secs_f32().min(MAX_STEP) * TIME_SCALE;
        self.time = self.time.rem_euclid(TIME_PERIOD);
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let pixel_height = area.height.saturating_mul(2);
        let center_x = f32::from(area.width) / 2.0;
        let center_y = f32::from(pixel_height) / 2.0;

        for y in 0..area.height {
            for x in 0..area.width {
                let top = self.pixel(x, y * 2, center_x, center_y);
                let bottom = self.pixel(x, y * 2 + 1, center_x, center_y);
                put_pixel_pair(buf, area.x + x, area.y + y, rgb(top), rgb(bottom));
            }
        }
    }
}

/// Interpolate [`STOPS`] into a 256 entry ramp that wraps around.
fn build_palette() -> [Rgb; 256] {
    let mut palette = [(0, 0, 0); 256];
    let segments = STOPS.len() - 1;

    for (level, slot) in palette.iter_mut().enumerate() {
        let position = level as f32 / 255.0 * segments as f32;
        let segment = (position as usize).min(segments - 1);
        *slot = lerp_rgb(
            STOPS[segment],
            STOPS[segment + 1],
            position - segment as f32,
        );
    }
    palette
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(seed: u64, area: Rect, frames: u32) -> Buffer {
        let mut plasma = Plasma::new(seed);
        plasma.resize(area);
        for _ in 0..frames {
            plasma.update(Duration::from_millis(33));
        }
        let mut buf = Buffer::empty(area);
        plasma.render(area, &mut buf);
        buf
    }

    #[test]
    fn sine_table_matches_the_real_sine() {
        let plasma = Plasma::new(0);
        for turns in [0.0, 0.1, 0.25, 0.5, 0.75, 0.99] {
            let expected = (turns * TAU).sin();
            let actual = plasma.sine_at(turns);
            assert!(
                (actual - expected).abs() < 0.01,
                "sine_at({turns}) = {actual}, expected {expected}"
            );
        }
    }

    #[test]
    fn sine_table_wraps_both_ways() {
        let plasma = Plasma::new(0);
        assert!((plasma.sine_at(1.25) - plasma.sine_at(0.25)).abs() < 1e-6);
        assert!((plasma.sine_at(-0.75) - plasma.sine_at(0.25)).abs() < 1e-6);
    }

    #[test]
    fn palette_is_cyclic() {
        let palette = build_palette();
        assert_eq!(palette[0], STOPS[0]);
        assert_eq!(palette[255], *STOPS.last().unwrap());
        assert_eq!(palette[0], palette[255], "the ramp has to close the loop");
    }

    #[test]
    fn deterministic_with_same_seed() {
        let area = Rect::new(0, 0, 40, 12);
        assert_eq!(advance(42, area, 30), advance(42, area, 30));
    }

    #[test]
    fn different_seeds_start_at_different_phases() {
        let area = Rect::new(0, 0, 40, 12);
        assert_ne!(advance(1, area, 5), advance(500, area, 5));
    }

    #[test]
    fn the_pattern_moves_over_time() {
        let area = Rect::new(0, 0, 40, 12);
        assert_ne!(advance(1, area, 1), advance(1, area, 120));
    }

    #[test]
    fn phase_stays_bounded_forever() {
        let mut plasma = Plasma::new(0);
        for _ in 0..10_000 {
            plasma.update(Duration::from_millis(33));
        }
        assert!((0.0..TIME_PERIOD).contains(&plasma.time), "{}", plasma.time);
    }

    #[test]
    fn fills_every_cell() {
        let area = Rect::new(0, 0, 20, 6);
        let buf = advance(3, area, 10);
        for y in 0..area.height {
            for x in 0..area.width {
                assert_eq!(buf[(x, y)].symbol(), "▀", "gap at {x},{y}");
            }
        }
    }

    #[test]
    fn stays_inside_the_area() {
        let area = Rect::new(3, 2, 20, 8);
        let mut plasma = Plasma::new(9);
        plasma.resize(area);
        plasma.update(Duration::from_millis(33));
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 16));
        plasma.render(area, &mut buf);

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
            let mut plasma = Plasma::new(1);
            plasma.resize(area);
            plasma.update(Duration::from_millis(33));
            let mut buf = Buffer::empty(area);
            plasma.render(area, &mut buf);
        }
    }
}
