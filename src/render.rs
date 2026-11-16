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

/// Wrap a coordinate that has strayed at most one step outside `limit`.
///
/// Only correct for an overshoot of one, which is all a neighbouring cell or
/// a one column drift can be. Worth having rather than `rem_euclid`: that is
/// an integer division, and in a per-pixel loop it shows up in the profile.
pub fn wrap_by_one(value: i32, limit: i32) -> usize {
    if value < 0 {
        (limit - 1).max(0) as usize
    } else if value >= limit {
        0
    } else {
        value as usize
    }
}

/// Perceived brightness of a colour, from 0 to 255.
///
/// Green counts for more than red and red for more than blue, roughly the
/// way an eye weighs them.
pub fn luma(color: Rgb) -> u8 {
    let weighted = u32::from(color.0) * 2 + u32::from(color.1) * 3 + u32::from(color.2);
    (weighted / 6) as u8
}

/// Interpolate colour stops into a 256 entry ramp, stops evenly spaced.
///
/// One stop gives a flat ramp; no stops gives black.
pub fn ramp(stops: &[Rgb]) -> [Rgb; 256] {
    let mut out = [(0, 0, 0); 256];
    match stops {
        [] => return out,
        [only] => {
            out.fill(*only);
            return out;
        }
        _ => {}
    }

    let segments = stops.len() - 1;
    for (level, slot) in out.iter_mut().enumerate() {
        let position = level as f32 / 255.0 * segments as f32;
        let segment = (position as usize).min(segments - 1);
        *slot = lerp_rgb(
            stops[segment],
            stops[segment + 1],
            position - segment as f32,
        );
    }
    out
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

/// Draw a half-block cell where either pixel may be empty.
///
/// Does nothing when both are empty, so whatever is behind keeps showing
/// through instead of being painted over in black.
pub fn put_pixel_pair_opt(buf: &mut Buffer, x: u16, y: u16, top: Option<Rgb>, bottom: Option<Rgb>) {
    /// Stand-in for a pixel with nothing in it.
    const DARK: Rgb = (0, 0, 0);

    if top.is_none() && bottom.is_none() {
        return;
    }
    put_pixel_pair(
        buf,
        x,
        y,
        rgb(top.unwrap_or(DARK)),
        rgb(bottom.unwrap_or(DARK)),
    );
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
    fn wrapping_by_one_matches_a_real_remainder() {
        for limit in 1..8 {
            for value in -1..=limit {
                assert_eq!(
                    wrap_by_one(value, limit),
                    value.rem_euclid(limit) as usize,
                    "value {value}, limit {limit}"
                );
            }
        }
    }

    #[test]
    fn luma_ranks_colours_by_brightness() {
        assert_eq!(luma((0, 0, 0)), 0);
        assert_eq!(luma((255, 255, 255)), 255);
        // Green reads brighter than red, red brighter than blue.
        assert!(luma((0, 255, 0)) > luma((255, 0, 0)));
        assert!(luma((255, 0, 0)) > luma((0, 0, 255)));
    }

    #[test]
    fn ramp_spans_its_stops() {
        let stops = [(0, 0, 0), (128, 64, 32), (255, 255, 255)];
        let table = ramp(&stops);
        assert_eq!(table[0], stops[0]);
        assert_eq!(table[255], stops[2]);
        // The middle stop lands mid-ramp, give or take one rounding step.
        let mid = table[128];
        assert!(
            mid.0.abs_diff(stops[1].0) <= 2
                && mid.1.abs_diff(stops[1].1) <= 2
                && mid.2.abs_diff(stops[1].2) <= 2,
            "{mid:?} is not near {:?}",
            stops[1]
        );
    }

    #[test]
    fn ramp_handles_degenerate_stop_lists() {
        assert_eq!(ramp(&[]), [(0, 0, 0); 256]);
        assert_eq!(ramp(&[(9, 8, 7)]), [(9, 8, 7); 256]);
        let two = ramp(&[(0, 0, 0), (255, 255, 255)]);
        assert_eq!(two[0], (0, 0, 0));
        assert_eq!(two[255], (255, 255, 255));
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
    fn optional_pixel_pair_skips_fully_empty_cells() {
        let area = Rect::new(0, 0, 3, 1);
        let mut buf = Buffer::empty(area);
        put_pixel_pair_opt(&mut buf, 0, 0, None, None);
        put_pixel_pair_opt(&mut buf, 1, 0, Some((10, 20, 30)), None);
        put_pixel_pair_opt(&mut buf, 2, 0, None, Some((40, 50, 60)));

        assert_eq!(buf[(0, 0)].symbol(), " ", "an empty pair draws nothing");
        assert_eq!(buf[(1, 0)].fg, Color::Rgb(10, 20, 30));
        assert_eq!(buf[(1, 0)].bg, Color::Rgb(0, 0, 0));
        assert_eq!(buf[(2, 0)].fg, Color::Rgb(0, 0, 0));
        assert_eq!(buf[(2, 0)].bg, Color::Rgb(40, 50, 60));
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
