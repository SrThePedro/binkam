//! A 3D starfield: stars stream out from the centre of the screen.

use std::time::Duration;

use rand::{RngExt, SeedableRng, rngs::StdRng};
use ratatui::{buffer::Buffer, layout::Rect};

use super::Animation;
use crate::render::{CELL_ASPECT, Rgb, lerp};

/// How fast stars travel towards the viewer, in depth units per second.
const SPEED: f32 = 0.5;

/// Depth at which a star has flown past the viewer and is recycled.
const NEAR_PLANE: f32 = 0.06;

/// One star per this many cells.
///
/// Generous on purpose: the perspective divide throws most of the field off
/// screen, so only roughly one star in five is ever visible.
const CELLS_PER_STAR: u32 = 8;

/// Star count bounds, so tiny terminals still twinkle and huge ones stay cheap.
const MIN_STARS: u32 = 4;
const MAX_STARS: u32 = 1400;

/// A single frame never advances the simulation by more than this, which keeps
/// the field sane after the machine wakes from sleep.
const MAX_STEP: f32 = 0.1;

/// Characters used from the faintest to the brightest star.
const RAMP: [char; 4] = ['.', '+', '*', '█'];

/// Colour of the most distant stars.
const FAR_COLOR: Rgb = (60, 70, 100);
/// Colour of stars about to fly past.
const NEAR_COLOR: Rgb = (235, 240, 255);

/// A star in normalised space: `x` and `y` in `-1.0..1.0`, `z` the depth.
#[derive(Debug, Clone, Copy)]
struct Star {
    x: f32,
    y: f32,
    z: f32,
}

impl Star {
    /// A star at a random depth, used when first filling the sky.
    fn scattered(rng: &mut StdRng) -> Self {
        Self {
            z: rng.random_range(NEAR_PLANE..1.0),
            ..Self::distant(rng)
        }
    }

    /// A star at the far plane, used when recycling one that flew past.
    fn distant(rng: &mut StdRng) -> Self {
        Self {
            x: rng.random_range(-1.0..1.0),
            y: rng.random_range(-1.0..1.0),
            z: 1.0,
        }
    }
}

/// The starfield animation.
#[derive(Debug)]
pub struct Starfield {
    rng: StdRng,
    stars: Vec<Star>,
}

impl Starfield {
    /// A starfield whose randomness is fully determined by `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            stars: Vec::new(),
        }
    }

    /// How many stars an area of this size should hold.
    fn star_count(area: Rect) -> usize {
        if area.is_empty() {
            return 0;
        }
        let cells = u32::from(area.width) * u32::from(area.height);
        (cells / CELLS_PER_STAR).clamp(MIN_STARS, MAX_STARS) as usize
    }
}

impl Animation for Starfield {
    fn name(&self) -> &'static str {
        "starfield"
    }

    fn resize(&mut self, area: Rect) {
        // Star coordinates are normalised, so only the population changes.
        // Destructuring splits the borrow: `stars` and `rng` are then two
        // independent mutable borrows instead of one of `self`.
        let Self { rng, stars } = self;
        stars.resize_with(Self::star_count(area), || Star::scattered(rng));
    }

    fn update(&mut self, dt: Duration) {
        let dt = dt.as_secs_f32().min(MAX_STEP);
        let Self { rng, stars } = self;

        for star in stars.iter_mut() {
            star.z -= SPEED * dt;
            if star.z <= NEAR_PLANE {
                *star = Star::distant(rng);
            }
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        let half_width = f32::from(area.width) / 2.0;
        let half_height = f32::from(area.height) / 2.0;

        for star in &self.stars {
            // Perspective divide: the smaller z is, the further from centre.
            let spread = 1.0 / star.z;
            let offset_x = star.x * spread * half_width;
            let offset_y = star.y * spread * half_width * CELL_ASPECT;

            let col = half_width + offset_x;
            let row = half_height + offset_y;
            if col < 0.0 || row < 0.0 {
                continue; // casting a negative float to u16 would clamp it to 0
            }

            // A star that flew far off screen saturates to u16::MAX and is culled.
            let x = area.x.saturating_add(col as u16);
            let y = area.y.saturating_add(row as u16);
            if x >= area.right() || y >= area.bottom() {
                continue;
            }

            // Closer stars are brighter and drawn with a heavier character.
            let nearness = (1.0 - star.z).clamp(0.0, 1.0);
            let step = (nearness * RAMP.len() as f32) as usize;
            let symbol = RAMP[step.min(RAMP.len() - 1)];

            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_char(symbol)
                    .set_fg(lerp(FAR_COLOR, NEAR_COLOR, nearness));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(seed: u64, area: Rect, frames: u32) -> Buffer {
        let mut field = Starfield::new(seed);
        field.resize(area);
        for _ in 0..frames {
            field.update(Duration::from_millis(33));
        }
        let mut buf = Buffer::empty(area);
        field.render(area, &mut buf);
        buf
    }

    #[test]
    fn deterministic_with_same_seed() {
        let area = Rect::new(0, 0, 40, 12);
        assert_eq!(advance(42, area, 30), advance(42, area, 30));
    }

    #[test]
    fn different_seeds_give_different_skies() {
        let area = Rect::new(0, 0, 60, 20);
        assert_ne!(advance(1, area, 10), advance(2, area, 10));
    }

    #[test]
    fn draws_something_on_a_normal_terminal() {
        let area = Rect::new(0, 0, 80, 24);
        let buf = advance(7, area, 15);
        let drawn = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
            .count();
        assert!(drawn > 10, "only {drawn} stars drawn");
    }

    #[test]
    fn stars_stay_inside_the_area() {
        // A non-zero origin catches any missing `area.x` / `area.y` offset.
        let area = Rect::new(5, 3, 30, 10);
        let mut field = Starfield::new(9);
        field.resize(area);
        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 20));

        for _ in 0..200 {
            field.update(Duration::from_millis(33));
            field.render(area, &mut buf);
        }

        for y in 0..20u16 {
            for x in 0..60u16 {
                let inside = area.contains((x, y).into());
                if !inside {
                    assert_eq!(
                        buf[(x, y)].symbol(),
                        " ",
                        "drew outside the area at {x},{y}"
                    );
                }
            }
        }
    }

    #[test]
    fn population_scales_with_the_area() {
        let mut field = Starfield::new(3);
        field.resize(Rect::new(0, 0, 200, 60));
        let big = field.stars.len();
        field.resize(Rect::new(0, 0, 20, 6));
        let small = field.stars.len();
        field.resize(Rect::new(0, 0, 4, 2));
        let tiny = field.stars.len();

        assert!(big > small && small > tiny, "{big} > {small} > {tiny}");
        assert_eq!(big, MAX_STARS as usize, "huge areas are capped");
        assert_eq!(tiny, MIN_STARS as usize, "tiny areas still get a few stars");
    }

    #[test]
    fn survives_tiny_and_zero_areas() {
        for area in [
            Rect::new(0, 0, 0, 0),
            Rect::new(0, 0, 1, 1),
            Rect::new(0, 0, 3, 2),
        ] {
            let mut field = Starfield::new(1);
            field.resize(area);
            field.update(Duration::from_millis(33));
            let mut buf = Buffer::empty(area);
            field.render(area, &mut buf);
        }
    }

    #[test]
    fn handles_shrinking_after_a_long_run() {
        let mut field = Starfield::new(1);
        field.resize(Rect::new(0, 0, 200, 60));
        field.update(Duration::from_millis(500));

        let small = Rect::new(0, 0, 20, 6);
        field.resize(small);
        let mut buf = Buffer::empty(small);
        field.render(small, &mut buf);
    }

    #[test]
    fn a_huge_frame_gap_does_not_teleport_stars() {
        let mut field = Starfield::new(5);
        field.resize(Rect::new(0, 0, 40, 12));
        // Pretend the machine slept for an hour.
        field.update(Duration::from_secs(3600));
        assert!(field.stars.iter().all(|s| s.z > NEAR_PLANE && s.z <= 1.0));
    }
}
