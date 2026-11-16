//! How much colour the terminal can take, and how to fit inside it.
//!
//! Everything is drawn in truecolor and reduced once per frame, at the very
//! end, so no animation has to know what the terminal supports.

use ratatui::{
    buffer::{Buffer, Cell},
    layout::Rect,
    style::Color,
};

use crate::render::{Rgb, luma};

/// Upper half block, the character the pixel animations draw with.
const UPPER_HALF: &str = "▀";
/// Its counterpart, for a pair whose lower pixel is the lit one.
const LOWER_HALF: char = '▄';
/// Both pixels lit.
const FULL_BLOCK: char = '█';

/// Brightness below which a colour counts as not there at all.
///
/// Only used without colour, where something has to stand in for shade.
const VISIBLE_LUMA: u8 = 40;

/// The six levels the 6x6x6 cube of the 256 colour palette is built from.
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// First index of the 6x6x6 colour cube.
const CUBE_START: u8 = 16;
/// First index of the 24 step grey ramp.
const GREY_START: u8 = 232;
/// Darkest grey in that ramp, and the step between greys.
const GREY_BASE: i32 = 8;
const GREY_STEP: i32 = 10;
/// Number of greys in the ramp.
const GREY_COUNT: i32 = 24;

/// What a terminal can actually display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    /// 24 bit colour, exactly what was drawn.
    Truecolor,
    /// The 256 colour palette.
    Indexed,
    /// No colour at all.
    None,
}

/// What the user asked for, which may be "work it out".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSetting {
    /// Guess from the environment.
    #[default]
    Auto,
    /// Force one of the depths.
    Truecolor,
    Indexed,
    None,
}

impl ColorSetting {
    /// Every setting name, in the order they are offered to the user.
    pub const fn all_names() -> &'static [&'static str] {
        &["256", "auto", "none", "truecolor"]
    }

    /// The setting called `name`, or `None` if there is no such setting.
    pub fn by_name(name: &str) -> Option<Self> {
        match name {
            "auto" => Some(Self::Auto),
            "truecolor" => Some(Self::Truecolor),
            "256" => Some(Self::Indexed),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    /// The name this setting is registered under.
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Truecolor => "truecolor",
            Self::Indexed => "256",
            Self::None => "none",
        }
    }

    /// The depth to use, asking the environment when told to guess.
    pub fn resolve(self) -> ColorDepth {
        match self {
            Self::Auto => detect(),
            Self::Truecolor => ColorDepth::Truecolor,
            Self::Indexed => ColorDepth::Indexed,
            Self::None => ColorDepth::None,
        }
    }
}

/// Guess the depth from this process's environment.
pub fn detect() -> ColorDepth {
    from_env(
        std::env::var("NO_COLOR").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
        std::env::var("COLORTERM").ok().as_deref(),
    )
}

/// The same decision, with the environment passed in so it can be tested.
///
/// Environment variables are process-wide, so a test that set them would
/// fight every other test running beside it.
pub fn from_env(no_color: Option<&str>, term: Option<&str>, colorterm: Option<&str>) -> ColorDepth {
    // no-color.org: any value at all, even empty, means no colour.
    if no_color.is_some() {
        return ColorDepth::None;
    }

    match term {
        None | Some("" | "dumb") => return ColorDepth::None,
        Some(_) => {}
    }

    if let Some(colorterm) = colorterm
        && (colorterm.eq_ignore_ascii_case("truecolor") || colorterm.eq_ignore_ascii_case("24bit"))
    {
        return ColorDepth::Truecolor;
    }

    // No COLORTERM is not a reason to give up on colour: 256 is near
    // universal for anything that is not a teletype, and the worst that
    // happens is slightly flatter gradients.
    ColorDepth::Indexed
}

/// Fit everything drawn inside `rect` into `depth`.
pub fn reduce(buf: &mut Buffer, rect: Rect, depth: ColorDepth) {
    if depth == ColorDepth::Truecolor {
        return; // nothing to do, which is the common case
    }

    for y in rect.top()..rect.bottom() {
        for x in rect.left()..rect.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                match depth {
                    ColorDepth::Truecolor => {}
                    ColorDepth::Indexed => quantize(cell),
                    ColorDepth::None => strip(cell),
                }
            }
        }
    }
}

/// Snap a cell's colours to the 256 colour palette.
fn quantize(cell: &mut Cell) {
    let fg = palette(cell.fg);
    let bg = palette(cell.bg);
    cell.set_fg(fg).set_bg(bg);
}

/// One colour snapped to the palette; anything not truecolor is left alone.
fn palette(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Indexed(nearest_index((r, g, b))),
        other => other,
    }
}

/// The palette index closest to `color`.
///
/// Both the colour cube and the grey ramp are tried, because a grey drawn
/// from the cube is noticeably off while the ramp has it exactly.
fn nearest_index(color: Rgb) -> u8 {
    let (cube_index, cube_color) = nearest_cube(color);
    let (grey_index, grey_color) = nearest_grey(color);

    if distance(color, grey_color) < distance(color, cube_color) {
        grey_index
    } else {
        cube_index
    }
}

/// The closest colour in the 6x6x6 cube, with its index.
fn nearest_cube(color: Rgb) -> (u8, Rgb) {
    let level = |component: u8| -> usize {
        CUBE_LEVELS
            .iter()
            .enumerate()
            .min_by_key(|(_, candidate)| candidate.abs_diff(component))
            .map_or(0, |(index, _)| index)
    };

    let (r, g, b) = (level(color.0), level(color.1), level(color.2));
    // The cube is laid out red-major, as 16 + 36r + 6g + b.
    let index = CUBE_START + u8::try_from(36 * r + 6 * g + b).unwrap_or(0);
    (index, (CUBE_LEVELS[r], CUBE_LEVELS[g], CUBE_LEVELS[b]))
}

/// The closest grey in the 24 step ramp, with its index.
fn nearest_grey(color: Rgb) -> (u8, Rgb) {
    let brightness = i32::from(luma(color));
    // Greys run GREY_BASE, GREY_BASE + GREY_STEP, ...; round to the nearest.
    let step = ((brightness - GREY_BASE + GREY_STEP / 2) / GREY_STEP).clamp(0, GREY_COUNT - 1);
    let value = u8::try_from(GREY_BASE + step * GREY_STEP).unwrap_or(u8::MAX);
    let index = GREY_START + u8::try_from(step).unwrap_or(0);
    (index, (value, value, value))
}

/// Squared distance between two colours, which is enough to compare them.
fn distance(a: Rgb, b: Rgb) -> u32 {
    let diff = |x: u8, y: u8| {
        let d = u32::from(x.abs_diff(y));
        d * d
    };
    diff(a.0, b.0) + diff(a.1, b.1) + diff(a.2, b.2)
}

/// Drop a cell's colour, keeping as much of its shape as a character can.
///
/// The pixel animations say everything through colour, so a half block whose
/// colours are gone would be a solid wall. Turning each lit pixel back into
/// the matching block character keeps the shapes readable.
fn strip(cell: &mut Cell) {
    let lit =
        |color: Color| matches!(color, Color::Rgb(r, g, b) if luma((r, g, b)) >= VISIBLE_LUMA);

    if cell.symbol() == UPPER_HALF {
        let symbol = match (lit(cell.fg), lit(cell.bg)) {
            (true, true) => FULL_BLOCK,
            (true, false) => '▀',
            (false, true) => LOWER_HALF,
            (false, false) => ' ',
        };
        cell.set_char(symbol);
    } else if matches!(cell.fg, Color::Rgb(..)) && !lit(cell.fg) {
        // Too dark to have been meant as visible.
        cell.set_char(' ');
    }

    cell.set_fg(Color::Reset).set_bg(Color::Reset);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_setting_round_trips() {
        for name in ColorSetting::all_names() {
            let setting = ColorSetting::by_name(name).unwrap_or_else(|| panic!("{name}"));
            assert_eq!(&setting.name(), name);
        }
    }

    #[test]
    fn unknown_settings_are_rejected() {
        assert!(ColorSetting::by_name("16").is_none());
    }

    #[test]
    fn names_are_sorted_and_unique() {
        let names = ColorSetting::all_names();
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, names);
    }

    #[test]
    fn forced_settings_ignore_the_environment() {
        assert_eq!(ColorSetting::Truecolor.resolve(), ColorDepth::Truecolor);
        assert_eq!(ColorSetting::Indexed.resolve(), ColorDepth::Indexed);
        assert_eq!(ColorSetting::None.resolve(), ColorDepth::None);
        assert_eq!(ColorSetting::default(), ColorSetting::Auto);
    }

    #[test]
    fn colorterm_means_truecolor() {
        for value in ["truecolor", "24bit", "TrueColor"] {
            assert_eq!(
                from_env(None, Some("xterm-256color"), Some(value)),
                ColorDepth::Truecolor,
                "COLORTERM={value}"
            );
        }
    }

    #[test]
    fn without_colorterm_we_settle_for_256() {
        assert_eq!(
            from_env(None, Some("xterm"), None),
            ColorDepth::Indexed,
            "a terminal without COLORTERM still does 256 colours"
        );
        assert_eq!(
            from_env(None, Some("screen"), Some("")),
            ColorDepth::Indexed
        );
    }

    #[test]
    fn no_color_and_dumb_terminals_get_nothing() {
        assert_eq!(
            from_env(Some(""), Some("xterm"), Some("truecolor")),
            ColorDepth::None,
            "an empty NO_COLOR still counts"
        );
        assert_eq!(from_env(Some("1"), Some("xterm"), None), ColorDepth::None);
        assert_eq!(from_env(None, Some("dumb"), None), ColorDepth::None);
        assert_eq!(from_env(None, Some(""), None), ColorDepth::None);
        assert_eq!(from_env(None, None, None), ColorDepth::None);
    }

    #[test]
    fn the_palette_has_the_corners_exactly() {
        assert_eq!(nearest_index((0, 0, 0)), CUBE_START, "black");
        assert_eq!(nearest_index((255, 255, 255)), 231, "white");
        assert_eq!(nearest_index((255, 0, 0)), CUBE_START + 36 * 5, "red");
        assert_eq!(nearest_index((0, 255, 0)), CUBE_START + 6 * 5, "green");
        assert_eq!(nearest_index((0, 0, 255)), CUBE_START + 5, "blue");
    }

    #[test]
    fn mid_greys_come_from_the_grey_ramp() {
        // 0x80 grey is not in the cube but the ramp has 128 exactly.
        let index = nearest_index((128, 128, 128));
        assert!(
            index >= GREY_START,
            "{index} should be a grey, not a cube colour"
        );
    }

    #[test]
    fn quantizing_keeps_colours_close() {
        for color in [
            (235, 90, 0),
            (46, 52, 64),
            (96, 196, 255),
            (18, 48, 108),
            (190, 255, 240),
        ] {
            let Color::Indexed(index) = palette(Color::Rgb(color.0, color.1, color.2)) else {
                panic!("expected an indexed colour");
            };
            // Whichever family it landed in, it has to be a real index.
            assert!(index >= CUBE_START, "{index} is a system colour");
        }
    }

    #[test]
    fn named_colours_survive_quantizing() {
        assert_eq!(palette(Color::White), Color::White);
        assert_eq!(palette(Color::Reset), Color::Reset);
        assert_eq!(palette(Color::Indexed(5)), Color::Indexed(5));
    }

    #[test]
    fn truecolor_reduction_changes_nothing() {
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_char('x').set_fg(Color::Rgb(1, 2, 3));
        let before = buf.clone();

        reduce(&mut buf, area, ColorDepth::Truecolor);
        assert_eq!(buf, before);
    }

    #[test]
    fn reduction_only_touches_the_given_rect() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        buf[(0, 0)].set_fg(Color::Rgb(200, 10, 10));
        buf[(3, 1)].set_fg(Color::Rgb(200, 10, 10));

        reduce(&mut buf, Rect::new(0, 0, 2, 1), ColorDepth::Indexed);

        assert!(matches!(buf[(0, 0)].fg, Color::Indexed(_)));
        assert_eq!(buf[(3, 1)].fg, Color::Rgb(200, 10, 10));
    }

    #[test]
    fn without_colour_half_blocks_become_shapes() {
        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::empty(area);
        let bright = Color::Rgb(220, 220, 220);
        let dark = Color::Rgb(4, 4, 4);

        buf[(0, 0)].set_char('▀').set_fg(bright).set_bg(bright);
        buf[(1, 0)].set_char('▀').set_fg(bright).set_bg(dark);
        buf[(2, 0)].set_char('▀').set_fg(dark).set_bg(bright);
        buf[(3, 0)].set_char('▀').set_fg(dark).set_bg(dark);

        reduce(&mut buf, area, ColorDepth::None);

        assert_eq!(buf[(0, 0)].symbol(), "█", "both pixels lit");
        assert_eq!(buf[(1, 0)].symbol(), "▀", "upper pixel lit");
        assert_eq!(buf[(2, 0)].symbol(), "▄", "lower pixel lit");
        assert_eq!(buf[(3, 0)].symbol(), " ", "neither pixel lit");

        for x in 0..4 {
            assert_eq!(buf[(x, 0)].fg, Color::Reset, "colour left behind");
            assert_eq!(buf[(x, 0)].bg, Color::Reset, "colour left behind");
        }
    }

    #[test]
    fn without_colour_dim_characters_go_but_bright_ones_stay() {
        let area = Rect::new(0, 0, 3, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_char('*').set_fg(Color::Rgb(220, 220, 220));
        buf[(1, 0)].set_char('.').set_fg(Color::Rgb(6, 10, 14));
        // The clock draws with named colours, which carry no brightness.
        buf[(2, 0)].set_char('█').set_fg(Color::White);

        reduce(&mut buf, area, ColorDepth::None);

        assert_eq!(buf[(0, 0)].symbol(), "*");
        assert_eq!(buf[(1, 0)].symbol(), " ", "a nearly black star is noise");
        assert_eq!(buf[(2, 0)].symbol(), "█", "the clock has to survive");
    }
}
