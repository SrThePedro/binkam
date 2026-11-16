//! Falling character rain, in the style of the film.

use std::time::Duration;

use rand::{RngExt, SeedableRng, rngs::StdRng};
use ratatui::{buffer::Buffer, layout::Rect};

use super::Animation;
use crate::render::{Rgb, lerp, rgb};

/// Characters the rain is made of: half-width katakana plus some ASCII, all
/// guaranteed to be a single column wide.
const CHARS: [char; 48] = [
    'ｱ', 'ｲ', 'ｳ', 'ｴ', 'ｵ', 'ｶ', 'ｷ', 'ｸ', 'ｹ', 'ｺ', 'ｻ', 'ｼ', 'ｽ', 'ｾ', 'ｿ', 'ﾀ', 'ﾁ', 'ﾂ', 'ﾃ',
    'ﾄ', 'ﾅ', 'ﾆ', 'ﾌ', 'ﾍ', 'ﾎ', 'ﾏ', 'ﾐ', 'ﾑ', 'ﾓ', 'ﾔ', 'ﾕ', 'ﾖ', 'ﾗ', 'ﾘ', 'ﾜ', '0', '1', '2',
    '3', '4', '5', '6', '7', '8', '9', ':', '=', '+',
];

/// Rows per second, slowest and fastest column.
const MIN_SPEED: f32 = 6.0;
const MAX_SPEED: f32 = 26.0;

/// Trail length bounds, in rows.
const MIN_TAIL: u16 = 4;
const MAX_TAIL: u16 = 18;

/// Longest step a single frame may take, so a long stall does not teleport
/// the rain to the bottom of the screen.
const MAX_STEP: f32 = 0.1;

/// Colour of the leading character.
const HEAD_COLOR: Rgb = (215, 255, 220);
/// Colour just behind the head.
const BRIGHT_COLOR: Rgb = (55, 225, 85);
/// Colour the tail fades out to.
const DARK_COLOR: Rgb = (0, 45, 18);

/// One falling stream.
#[derive(Debug, Clone, Copy)]
struct Column {
    /// Row of the leading character. Starts negative so streams fall in.
    head: f32,
    /// Rows per second.
    speed: f32,
    /// How many characters trail behind the head.
    tail: u16,
}

/// The character rain animation.
#[derive(Debug)]
pub struct Matrix {
    rng: StdRng,
    columns: Vec<Column>,
    /// The character sitting in every cell, row-major, `width * height` long.
    ///
    /// Kept here rather than picked while drawing, so a character stays put
    /// until a head passes over it again instead of flickering every frame.
    glyphs: Vec<char>,
    width: u16,
    height: u16,
}

impl Matrix {
    /// A rain whose randomness is fully determined by `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            columns: Vec::new(),
            glyphs: Vec::new(),
            width: 0,
            height: 0,
        }
    }

    /// A fresh stream, starting somewhere above the top edge.
    fn spawn(rng: &mut StdRng, height: u16) -> Column {
        Column {
            head: -rng.random_range(0.0..f32::from(height).max(1.0)),
            speed: rng.random_range(MIN_SPEED..MAX_SPEED),
            tail: rng.random_range(MIN_TAIL..=MAX_TAIL),
        }
    }

    /// Pick a random character from [`CHARS`].
    fn random_char(rng: &mut StdRng) -> char {
        CHARS[rng.random_range(0..CHARS.len())]
    }
}

impl Animation for Matrix {
    fn name(&self) -> &'static str {
        "matrix"
    }

    fn resize(&mut self, area: Rect) {
        let Self {
            rng,
            columns,
            glyphs,
            width,
            height,
        } = self;

        *width = area.width;
        *height = area.height;

        columns.clear();
        glyphs.clear();
        if area.is_empty() {
            return;
        }

        columns.resize_with(usize::from(area.width), || Self::spawn(rng, area.height));
        let cells = usize::from(area.width) * usize::from(area.height);
        glyphs.resize_with(cells, || Self::random_char(rng));
    }

    fn update(&mut self, dt: Duration) {
        let dt = dt.as_secs_f32().min(MAX_STEP);
        let Self {
            rng,
            columns,
            glyphs,
            width,
            height,
        } = self;

        if *width == 0 || *height == 0 {
            return;
        }
        let rows = i32::from(*height);

        for (x, column) in columns.iter_mut().enumerate() {
            let before = column.head.floor() as i32;
            column.head += column.speed * dt;
            let after = column.head.floor() as i32;

            // Every row the head crossed gets a new character.
            for row in (before + 1)..=after {
                if (0..rows).contains(&row) {
                    let index = row as usize * usize::from(*width) + x;
                    glyphs[index] = Self::random_char(rng);
                }
            }

            // Once the whole trail has left the screen, start over.
            if after - i32::from(column.tail) > rows {
                *column = Self::spawn(rng, *height);
            }
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }

        for (x, column) in self.columns.iter().enumerate() {
            let Ok(x) = u16::try_from(x) else { continue };
            if x >= area.width {
                break;
            }
            let head = column.head.floor() as i32;

            for step in 0..column.tail {
                let row = head - i32::from(step);
                if row < 0 || row >= i32::from(self.height) {
                    continue;
                }
                let row = row as u16;
                if row >= area.height {
                    continue;
                }

                let color = if step == 0 {
                    rgb(HEAD_COLOR)
                } else {
                    let fade = f32::from(step) / f32::from(column.tail);
                    lerp(BRIGHT_COLOR, DARK_COLOR, fade)
                };

                let glyph =
                    self.glyphs[usize::from(row) * usize::from(self.width) + usize::from(x)];
                if let Some(cell) = buf.cell_mut((area.x + x, area.y + row)) {
                    cell.set_char(glyph).set_fg(color);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn advance(seed: u64, area: Rect, frames: u32) -> Buffer {
        let mut rain = Matrix::new(seed);
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
    fn fills_a_good_part_of_the_screen() {
        let area = Rect::new(0, 0, 80, 24);
        let buf = advance(7, area, 60);
        let drawn = (0..area.height)
            .flat_map(|y| (0..area.width).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() != " ")
            .count();
        assert!(drawn > 200, "only {drawn} cells drawn");
    }

    #[test]
    fn characters_do_not_flicker_between_frames() {
        let area = Rect::new(0, 0, 20, 30);
        let mut rain = Matrix::new(4);
        rain.resize(area);
        // A slow column keeps its head inside one row across these two frames.
        rain.columns[0] = Column {
            head: 10.0,
            speed: 1.0,
            tail: 6,
        };

        let mut first = Buffer::empty(area);
        rain.render(area, &mut first);
        rain.update(Duration::from_millis(16));
        let mut second = Buffer::empty(area);
        rain.render(area, &mut second);

        assert_eq!(
            first[(0, 8)].symbol(),
            second[(0, 8)].symbol(),
            "a character behind the head must not change"
        );
    }

    #[test]
    fn stays_inside_the_area() {
        let area = Rect::new(4, 2, 20, 8);
        let mut rain = Matrix::new(9);
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
    fn streams_are_recycled_instead_of_running_away() {
        let mut rain = Matrix::new(2);
        let area = Rect::new(0, 0, 30, 10);
        rain.resize(area);
        for _ in 0..600 {
            rain.update(Duration::from_millis(33));
        }
        assert!(
            rain.columns
                .iter()
                .all(|c| c.head < f32::from(area.height) + f32::from(MAX_TAIL)),
            "a column ran off the bottom without respawning"
        );
    }

    #[test]
    fn survives_tiny_and_zero_areas() {
        for area in [
            Rect::new(0, 0, 0, 0),
            Rect::new(0, 0, 1, 1),
            Rect::new(0, 0, 3, 2),
        ] {
            let mut rain = Matrix::new(1);
            rain.resize(area);
            rain.update(Duration::from_millis(33));
            let mut buf = Buffer::empty(area);
            rain.render(area, &mut buf);
        }
    }

    #[test]
    fn handles_shrinking_after_a_long_run() {
        let mut rain = Matrix::new(1);
        rain.resize(Rect::new(0, 0, 200, 60));
        rain.update(Duration::from_millis(500));

        let small = Rect::new(0, 0, 20, 6);
        rain.resize(small);
        let mut buf = Buffer::empty(small);
        rain.render(small, &mut buf);
        assert_eq!(rain.glyphs.len(), 20 * 6);
    }
}
