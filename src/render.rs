//! Drawing helpers shared by the clock and the animations.

use ratatui::{buffer::Buffer, layout::Rect, style::Color};

/// An RGB triple, kept as plain numbers so colours can be blended before
/// they ever become a [`Color`].
pub type Rgb = (u8, u8, u8);

/// Terminal cells are roughly twice as tall as they are wide.
///
/// Multiply vertical distances by this to keep circles looking round.
pub const CELL_ASPECT: f32 = 0.5;

/// Column count of `text`, saturating instead of wrapping.
///
/// Counts characters, not bytes, so Turkish letters measure as one column.
pub fn text_cols(text: &str) -> u16 {
    u16::try_from(text.chars().count()).unwrap_or(u16::MAX)
}

/// An RGB triple as a ratatui colour.
pub fn rgb(color: Rgb) -> Color {
    Color::Rgb(color.0, color.1, color.2)
}

/// Blend `a` towards `b`; `t` is clamped to `0.0..=1.0`.
pub fn lerp_rgb(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let mix = |from: u8, to: u8| {
        let from = f32::from(from);
        (from + (f32::from(to) - from) * t)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Same as [`lerp_rgb`], but ready to hand to ratatui.
pub fn lerp(a: Rgb, b: Rgb, t: f32) -> Color {
    let (r, g, b) = lerp_rgb(a, b, t);
    Color::Rgb(r, g, b)
}

/// Scale an RGB triple towards black. `factor` above 1.0 brightens.
pub fn scale_rgb(color: Rgb, factor: f32) -> Rgb {
    let mul = |c: u8| (f32::from(c) * factor).round().clamp(0.0, 255.0) as u8;
    (mul(color.0), mul(color.1), mul(color.2))
}

/// Draw two stacked pixels inside one cell with an upper half block.
///
/// The cell grid is `area.width` by `area.height * 2` pixels: `top` is the
/// even pixel row, `bottom` the odd one.
pub fn put_pixel_pair(buf: &mut Buffer, x: u16, y: u16, top: Color, bottom: Color) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_char('▀').set_fg(top).set_bg(bottom);
    }
}

/// Fade everything already drawn inside `rect` towards black.
///
/// `keep` is the fraction of brightness that survives, so `0.25` leaves a
/// quarter. Used to darken the animation behind the clock without hiding it.
/// Colours that are not [`Color::Rgb`] are left alone, since there is no
/// meaningful way to dim a named terminal colour.
pub fn fade_to_black(buf: &mut Buffer, rect: Rect, keep: f32) {
    for y in rect.top()..rect.bottom() {
        for x in rect.left()..rect.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                let fg = faded(cell.fg, keep);
                let bg = faded(cell.bg, keep);
                cell.set_fg(fg).set_bg(bg);
            }
        }
    }
}

/// A single faded colour, see [`fade_to_black`].
fn faded(color: Color, keep: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            let (r, g, b) = scale_rgb((r, g, b), keep);
            Color::Rgb(r, g, b)
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_passes_components_through() {
        assert_eq!(rgb((1, 2, 3)), Color::Rgb(1, 2, 3));
    }

    #[test]
    fn lerp_hits_both_ends() {
        let a = (0, 10, 20);
        let b = (200, 100, 60);
        assert_eq!(lerp_rgb(a, b, 0.0), a);
        assert_eq!(lerp_rgb(a, b, 1.0), b);
        assert_eq!(lerp_rgb(a, b, 0.5), (100, 55, 40));
    }

    #[test]
    fn lerp_clamps_out_of_range_factors() {
        let a = (0, 0, 0);
        let b = (100, 100, 100);
        assert_eq!(lerp_rgb(a, b, -5.0), a);
        assert_eq!(lerp_rgb(a, b, 5.0), b);
        assert!(lerp_rgb(a, b, f32::NAN).0 <= 100);
    }

    #[test]
    fn scaling_saturates_instead_of_wrapping() {
        assert_eq!(scale_rgb((200, 100, 50), 0.5), (100, 50, 25));
        assert_eq!(scale_rgb((200, 100, 50), 10.0), (255, 255, 255));
        assert_eq!(scale_rgb((200, 100, 50), 0.0), (0, 0, 0));
    }

    #[test]
    fn pixel_pair_uses_an_upper_half_block() {
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        put_pixel_pair(&mut buf, 0, 0, Color::Rgb(1, 2, 3), Color::Rgb(4, 5, 6));
        assert_eq!(buf[(0, 0)].symbol(), "▀");
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(1, 2, 3));
        assert_eq!(buf[(0, 0)].bg, Color::Rgb(4, 5, 6));
    }

    #[test]
    fn pixel_pair_ignores_out_of_bounds_writes() {
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        put_pixel_pair(&mut buf, 99, 99, Color::Red, Color::Blue);
    }

    #[test]
    fn fading_dims_only_rgb_colours() {
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_fg(Color::Rgb(200, 200, 200));
        buf[(1, 0)].set_fg(Color::Red);

        fade_to_black(&mut buf, area, 0.25);

        assert_eq!(buf[(0, 0)].fg, Color::Rgb(50, 50, 50));
        assert_eq!(buf[(1, 0)].fg, Color::Red);
    }

    #[test]
    fn text_cols_counts_characters_not_bytes() {
        assert_eq!(text_cols("Kasım"), 5);
        assert_eq!(text_cols(""), 0);
    }
}
