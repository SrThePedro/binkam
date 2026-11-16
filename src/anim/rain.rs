//! Slanted rain: streaks fall on a diagonal and splash where they land.

use std::time::Duration;

use rand::{RngExt, SeedableRng, rngs::StdRng};
use ratatui::{buffer::Buffer, layout::Rect};

use super::Animation;
use crate::render::{Rgb, lerp};

/// Body of a streak. A diagonal glyph reads as continuous at the slant below.
const STREAK: char = '╲';
/// Mark left where a streak hits the bottom.
const SPLASH: char = '·';

/// Columns a streak moves right for every row it falls.
const SLANT: f32 = 1.0;

/// Falling speed bounds, in rows per second.
const MIN_SPEED: f32 = 24.0;
const MAX_SPEED: f32 = 52.0;

/// Streak length bounds, in rows.
const MIN_LENGTH: u16 = 3;
const MAX_LENGTH: u16 = 8;

/// How long a splash stays visible, in seconds.
const SPLASH_LIFE: f32 = 0.3;

/// One streak per this many cells.
const CELLS_PER_DROP: u32 = 46;

/// Streak count bounds.
const MIN_DROPS: u32 = 3;
const MAX_DROPS: u32 = 500;

/// Longest frame gap the animation reacts to.
const MAX_STEP: f32 = 0.1;

/// Colour of the leading end of a streak.
const HEAD_COLOR: Rgb = (198, 224, 255);
/// Colour the trailing end fades to.
const TAIL_COLOR: Rgb = (28, 52, 92);
/// Colour of a fresh splash.
const SPLASH_COLOR: Rgb = (132, 178, 228);
/// Colour a splash fades out to.
const SPLASH_FADE: Rgb = (14, 26, 48);

/// One falling streak.
#[derive(Debug, Clone, Copy)]
struct Drop {
    /// Column of the leading end.
    x: f32,
    /// Row of the leading end, growing downwards.
    y: f32,
    /// Rows per second.
    speed: f32,
    /// Rows the streak covers behind its head.
    length: u16,
}

impl Drop {
    /// A streak somewhere in the air, used when first filling the sky.
    fn scattered(rng: &mut StdRng, area: Rect) -> Self {
        Self {
            y: rng.random_range(0.0..f32::from(area.height).max(1.0)),
            ..Self::above(rng, area)
        }
    }

    /// A streak above the top edge, used when recycling one that landed.
    fn above(rng: &mut StdRng, area: Rect) -> Self {
        Self {
            x: rng.random_range(0.0..f32::from(area.width).max(1.0)),
            y: -rng.random_range(0.0..f32::from(area.height).max(1.0)),
            speed: rng.random_range(MIN_SPEED..MAX_SPEED),
            length: rng.random_range(MIN_LENGTH..=MAX_LENGTH),
        }
    }
}

/// A mark left where a streak hit the bottom.
#[derive(Debug, Clone, Copy)]
struct Burst {
    x: u16,
    y: u16,
    /// Seconds left before it disappears.
    life: f32,
}

/// The rain animation.
#[derive(Debug)]
pub struct Rain {
    rng: StdRng,
    drops: Vec<Drop>,
    /// Active splashes. Capacity is reserved up front so pushing one never
    /// allocates mid-animation.
    bursts: Vec<Burst>,
    area: Rect,
}

impl Rain {
    /// Rain whose randomness is fully determined by `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            drops: Vec::new(),
            bursts: Vec::new(),
            area: Rect::ZERO,
        }
    }

    /// How many streaks an area of this size should hold.
    fn drop_count(area: Rect) -> usize {
        if area.is_empty() {
            return 0;
        }
        let cells = u32::from(area.width) * u32::from(area.height);
        (cells / CELLS_PER_DROP).clamp(MIN_DROPS, MAX_DROPS) as usize
    }
}

impl Animation for Rain {
    fn name(&self) -> &'static str {
        "rain"
    }

    fn resize(&mut self, area: Rect) {
        let Self {
            rng, drops, bursts, ..
        } = self;

        drops.clear();
        drops.resize_with(Self::drop_count(area), || Drop::scattered(rng, area));
        bursts.clear();
        bursts.reserve(drops.len());
        self.area = area;
    }

    fn update(&mut self, dt: Duration) {
        let dt = dt.as_secs_f32().min(MAX_STEP);
        let Self {
            rng,
            drops,
            bursts,
            area,
        } = self;
        if area.is_empty() {
            return;
        }

        let floor = f32::from(area.height);
        let width = f32::from(area.width);
        let capacity = bursts.capacity();

        for drop in drops.iter_mut() {
            let was_above = drop.y < floor;
            let fallen = drop.speed * dt;
            drop.y += fallen;
            drop.x += SLANT * fallen;

            // The head crossed the bottom edge on this frame.
            if was_above && drop.y >= floor && bursts.len() < capacity {
                bursts.push(Burst {
                    x: drop.x.rem_euclid(width) as u16,
                    y: area.height - 1,
                    life: SPLASH_LIFE,
                });
            }

            // Recycle once the whole streak has left the screen.
            if drop.y - f32::from(drop.length) > floor {
                *drop = Drop::above(rng, *area);
            }
        }

        for burst in bursts.iter_mut() {
            burst.life -= dt;
        }
        bursts.retain(|burst| burst.life > 0.0);
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let width = f32::from(area.width);

        for drop in &self.drops {
            for step in 0..drop.length {
                let row = drop.y - f32::from(step);
                if row < 0.0 {
                    break; // the rest of the streak is above the top edge
                }
                let row = row as u16;
                if row >= area.height {
                    continue;
                }

                let column = (drop.x - SLANT * f32::from(step)).rem_euclid(width) as u16;
                if column >= area.width {
                    continue;
                }

                let fade = f32::from(step) / f32::from(drop.length);
                if let Some(cell) = buf.cell_mut((area.x + column, area.y + row)) {
                    cell.set_char(STREAK)
                        .set_fg(lerp(HEAD_COLOR, TAIL_COLOR, fade));
                }
            }
        }

        for burst in &self.bursts {
            if burst.x >= area.width || burst.y >= area.height {
                continue;
            }
            let fade = 1.0 - (burst.life / SPLASH_LIFE).clamp(0.0, 1.0);
            if let Some(cell) = buf.cell_mut((area.x + burst.x, area.y + burst.y)) {
                cell.set_char(SPLASH)
                    .set_fg(lerp(SPLASH_COLOR, SPLASH_FADE, fade));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(seed: u64, area: Rect, frames: u32) -> Buffer {
        let mut rain = Rain::new(seed);
        rain.resize(area);
        for _ in 0..frames {
            rain.update(Duration::from_millis(33));
        }
        let mut buf = Buffer::empty(area);
        rain.render(area, &mut buf);
        buf
    }

    #[test]
    fn deterministic_with_same_seed() {
        let area = Rect::new(0, 0, 40, 12);
        assert_eq!(advance(42, area, 30), advance(42, area, 30));
    }

    #[test]
    fn different_seeds_give_different_rain() {
        let area = Rect::new(0, 0, 60, 20);
        assert_ne!(advance(1, area, 20), advance(2, area, 20));
    }

    #[test]
    fn draws_streaks_on_a_normal_terminal() {
        let area = Rect::new(0, 0, 80, 24);
        let buf = advance(7, area, 60);
        let streaks = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol().starts_with(STREAK))
            .count();
        assert!(streaks > 30, "only {streaks} streak cells drawn");
    }

    #[test]
    fn landing_leaves_a_splash() {
        let area = Rect::new(0, 0, 40, 12);
        let mut rain = Rain::new(5);
        rain.resize(area);

        let mut seen = false;
        for _ in 0..120 {
            rain.update(Duration::from_millis(33));
            seen |= !rain.bursts.is_empty();
        }
        assert!(seen, "no streak ever hit the bottom");
    }

    #[test]
    fn splashes_expire() {
        let mut rain = Rain::new(5);
        rain.resize(Rect::new(0, 0, 40, 12));
        // Drop the streaks so no new splash can appear and confuse the count.
        rain.drops.clear();
        rain.bursts.clear();
        rain.bursts.push(Burst {
            x: 3,
            y: 5,
            life: 0.05,
        });

        rain.update(Duration::from_millis(33));
        assert_eq!(rain.bursts.len(), 1, "it should still be fading");
        rain.update(Duration::from_millis(33));
        assert!(rain.bursts.is_empty(), "it should be gone");
    }

    #[test]
    fn splashes_never_outgrow_their_capacity() {
        let area = Rect::new(0, 0, 80, 24);
        let mut rain = Rain::new(5);
        rain.resize(area);
        let capacity = rain.bursts.capacity();
        for _ in 0..600 {
            rain.update(Duration::from_millis(33));
            assert!(rain.bursts.len() <= capacity, "splash list grew");
        }
    }

    #[test]
    fn streaks_are_recycled() {
        let area = Rect::new(0, 0, 30, 10);
        let mut rain = Rain::new(2);
        rain.resize(area);
        for _ in 0..600 {
            rain.update(Duration::from_millis(33));
        }
        assert!(
            rain.drops
                .iter()
                .all(|d| d.y - f32::from(d.length) <= f32::from(area.height)),
            "a streak fell through the floor"
        );
    }

    #[test]
    fn stays_inside_the_area() {
        let area = Rect::new(4, 2, 20, 8);
        let mut rain = Rain::new(9);
        rain.resize(area);
        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 16));

        for _ in 0..200 {
            rain.update(Duration::from_millis(33));
            rain.render(area, &mut buf);
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
            let mut rain = Rain::new(1);
            rain.resize(area);
            rain.update(Duration::from_millis(33));
            let mut buf = Buffer::empty(area);
            rain.render(area, &mut buf);
        }
    }

    #[test]
    fn handles_shrinking_after_a_long_run() {
        let mut rain = Rain::new(1);
        rain.resize(Rect::new(0, 0, 200, 60));
        rain.update(Duration::from_millis(500));

        let small = Rect::new(0, 0, 20, 6);
        rain.resize(small);
        let mut buf = Buffer::empty(small);
        rain.render(small, &mut buf);
    }
}
