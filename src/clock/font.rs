//! Block-character fonts for the big clock.
//!
//! Every glyph is a list of rows made of block characters and spaces. Spaces
//! are never drawn, so whatever is behind the clock — an animation — keeps
//! showing through.
//!
//! A font is picked by name and drawn at a whole-number size, where each
//! glyph pixel becomes a `scale` by `scale` square of cells. Scaling both
//! axes by the same amount keeps every font looking like itself, only bigger.

/// A single glyph: one string per row, all of them the same length.
pub type Glyph = &'static [&'static str];

/// Font used when nothing asks for another one.
pub const DEFAULT: &str = "block";

/// Largest size multiplier on offer.
pub const MAX_SCALE: u8 = 4;

/// Blank columns inserted between two neighbouring glyphs, before scaling.
pub const GLYPH_GAP: u16 = 1;

/// Digits for the `block` font, 6 columns wide.
const BLOCK_DIGITS: [Glyph; 10] = [
    &["██████", "██  ██", "██  ██", "██  ██", "██████"], // 0
    &["    ██", "    ██", "    ██", "    ██", "    ██"], // 1
    &["██████", "    ██", "██████", "██    ", "██████"], // 2
    &["██████", "    ██", "██████", "    ██", "██████"], // 3
    &["██  ██", "██  ██", "██████", "    ██", "    ██"], // 4
    &["██████", "██    ", "██████", "    ██", "██████"], // 5
    &["██████", "██    ", "██████", "██  ██", "██████"], // 6
    &["██████", "    ██", "    ██", "    ██", "    ██"], // 7
    &["██████", "██  ██", "██████", "██  ██", "██████"], // 8
    &["██████", "██  ██", "██████", "    ██", "██████"], // 9
];

/// Separator for the `block` font.
const BLOCK_COLON: Glyph = &["  ", "██", "  ", "██", "  "];

/// Digits for the `slim` font, 4 columns wide.
const SLIM_DIGITS: [Glyph; 10] = [
    &["████", "█  █", "█  █", "█  █", "████"], // 0
    &["   █", "   █", "   █", "   █", "   █"], // 1
    &["████", "   █", "████", "█   ", "████"], // 2
    &["████", "   █", "████", "   █", "████"], // 3
    &["█  █", "█  █", "████", "   █", "   █"], // 4
    &["████", "█   ", "████", "   █", "████"], // 5
    &["████", "█   ", "████", "█  █", "████"], // 6
    &["████", "   █", "   █", "   █", "   █"], // 7
    &["████", "█  █", "████", "█  █", "████"], // 8
    &["████", "█  █", "████", "   █", "████"], // 9
];

/// Separator for the `slim` font.
const SLIM_COLON: Glyph = &[" ", "█", " ", "█", " "];

/// Digits for the `tall` font, 6 columns wide.
const TALL_DIGITS: [Glyph; 10] = [
    &[
        "██████",
        "██  ██",
        "██  ██",
        "██  ██",
        "██  ██",
        "██  ██",
        "██████",
    ], // 0
    &[
        "    ██",
        "    ██",
        "    ██",
        "    ██",
        "    ██",
        "    ██",
        "    ██",
    ], // 1
    &[
        "██████",
        "    ██",
        "    ██",
        "██████",
        "██    ",
        "██    ",
        "██████",
    ], // 2
    &[
        "██████",
        "    ██",
        "    ██",
        "██████",
        "    ██",
        "    ██",
        "██████",
    ], // 3
    &[
        "██  ██",
        "██  ██",
        "██  ██",
        "██████",
        "    ██",
        "    ██",
        "    ██",
    ], // 4
    &[
        "██████",
        "██    ",
        "██    ",
        "██████",
        "    ██",
        "    ██",
        "██████",
    ], // 5
    &[
        "██████",
        "██    ",
        "██    ",
        "██████",
        "██  ██",
        "██  ██",
        "██████",
    ], // 6
    &[
        "██████",
        "    ██",
        "    ██",
        "    ██",
        "    ██",
        "    ██",
        "    ██",
    ], // 7
    &[
        "██████",
        "██  ██",
        "██  ██",
        "██████",
        "██  ██",
        "██  ██",
        "██████",
    ], // 8
    &[
        "██████",
        "██  ██",
        "██  ██",
        "██████",
        "    ██",
        "    ██",
        "██████",
    ], // 9
];

/// Separator for the `tall` font.
const TALL_COLON: Glyph = &["  ", "  ", "██", "  ", "██", "  ", "  "];

/// Digits for the `shadow` font, 7 columns wide.
const SHADOW_DIGITS: [Glyph; 10] = [
    &[
        "██████ ",
        "██▒▒██▒",
        "██▒ ██▒",
        "██▒ ██▒",
        "██████▒",
        " ▒▒▒▒▒▒",
    ], // 0
    &[
        "    ██ ",
        "    ██▒",
        "    ██▒",
        "    ██▒",
        "    ██▒",
        "     ▒▒",
    ], // 1
    &[
        "██████ ",
        " ▒▒▒██▒",
        "██████▒",
        "██▒▒▒▒▒",
        "██████ ",
        " ▒▒▒▒▒▒",
    ], // 2
    &[
        "██████ ",
        " ▒▒▒██▒",
        "██████▒",
        " ▒▒▒██▒",
        "██████▒",
        " ▒▒▒▒▒▒",
    ], // 3
    &[
        "██  ██ ",
        "██▒ ██▒",
        "██████▒",
        " ▒▒▒██▒",
        "    ██▒",
        "     ▒▒",
    ], // 4
    &[
        "██████ ",
        "██▒▒▒▒▒",
        "██████ ",
        " ▒▒▒██▒",
        "██████▒",
        " ▒▒▒▒▒▒",
    ], // 5
    &[
        "██████ ",
        "██▒▒▒▒▒",
        "██████ ",
        "██▒▒██▒",
        "██████▒",
        " ▒▒▒▒▒▒",
    ], // 6
    &[
        "██████ ",
        " ▒▒▒██▒",
        "    ██▒",
        "    ██▒",
        "    ██▒",
        "     ▒▒",
    ], // 7
    &[
        "██████ ",
        "██▒▒██▒",
        "██████▒",
        "██▒▒██▒",
        "██████▒",
        " ▒▒▒▒▒▒",
    ], // 8
    &[
        "██████ ",
        "██▒▒██▒",
        "██████▒",
        " ▒▒▒██▒",
        "██████▒",
        " ▒▒▒▒▒▒",
    ], // 9
];

/// Separator for the `shadow` font.
const SHADOW_COLON: Glyph = &["   ", "██ ", " ▒▒", "██ ", " ▒▒", "   "];

/// One complete set of glyphs.
///
/// The fields are private and the only way to get a `Font` is
/// [`Font::by_name`], so every font in the program comes from the table
/// below and the invariants its tests check hold for all of them.
#[derive(Debug)]
pub struct Font {
    name: &'static str,
    digits: &'static [Glyph; 10],
    colon: Glyph,
}

/// Every font, in the order they are offered to the user.
const FONTS: [Font; 4] = [
    Font {
        name: "block",
        digits: &BLOCK_DIGITS,
        colon: BLOCK_COLON,
    },
    Font {
        name: "shadow",
        digits: &SHADOW_DIGITS,
        colon: SHADOW_COLON,
    },
    Font {
        name: "slim",
        digits: &SLIM_DIGITS,
        colon: SLIM_COLON,
    },
    Font {
        name: "tall",
        digits: &TALL_DIGITS,
        colon: TALL_COLON,
    },
];

impl Font {
    /// Every font name, in the order they are offered to the user.
    pub fn all_names() -> Vec<&'static str> {
        FONTS.iter().map(Font::name).collect()
    }

    /// The font called `name`, or `None` if there is no such font.
    pub fn by_name(name: &str) -> Option<&'static Self> {
        FONTS.iter().find(|font| font.name == name)
    }

    /// The font used when nothing asks for another one.
    pub fn fallback() -> &'static Self {
        // The table is a constant and `DEFAULT` names one of its entries,
        // which `the_default_font_exists` guards.
        match Self::by_name(DEFAULT) {
            Some(font) => font,
            None => &FONTS[0],
        }
    }

    /// The name this font is registered under.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The glyph for `c`, or `None` if the font has no such character.
    pub fn glyph(&self, c: char) -> Option<Glyph> {
        match c {
            ':' => Some(self.colon),
            // `to_digit` keeps us from hand-rolling the ASCII arithmetic.
            _ => c.to_digit(10).map(|d| self.digits[d as usize]),
        }
    }

    /// Rows in every glyph of this font, before scaling.
    pub fn height(&self) -> u16 {
        // Every glyph of a font has the same height; `glyphs_are_rectangular`
        // guards that.
        u16::try_from(self.digits[0].len()).unwrap_or(u16::MAX)
    }

    /// Rows a glyph of this font takes at `scale`.
    pub fn scaled_height(&self, scale: u8) -> u16 {
        self.height().saturating_mul(u16::from(scale))
    }

    /// Total width of `text` once rendered at `scale`, gaps included.
    ///
    /// Characters the font does not know are skipped.
    pub fn text_width(&self, text: &str, scale: u8) -> u16 {
        let scale = u16::from(scale);
        let mut width = 0u16;
        let mut drawn = 0u16;
        for glyph in text.chars().filter_map(|c| self.glyph(c)) {
            width = width.saturating_add(glyph_width(glyph).saturating_mul(scale));
            drawn = drawn.saturating_add(1);
        }
        // n glyphs have n-1 gaps between them.
        width.saturating_add(
            drawn
                .saturating_sub(1)
                .saturating_mul(GLYPH_GAP.saturating_mul(scale)),
        )
    }
}

/// Column count of a glyph, taken from its first row.
pub fn glyph_width(glyph: Glyph) -> u16 {
    // Every row of a glyph has the same length; `glyphs_are_rectangular`
    // guards that.
    u16::try_from(glyph[0].chars().count()).unwrap_or(u16::MAX)
}

/// A size multiplier that this font can actually be drawn at.
pub fn clamp_scale(scale: u8) -> u8 {
    scale.clamp(1, MAX_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every glyph of every font, the separators included.
    fn glyphs() -> impl Iterator<Item = (&'static str, Glyph)> {
        FONTS.iter().flat_map(|font| {
            font.digits
                .iter()
                .copied()
                .chain([font.colon])
                .map(|glyph| (font.name, glyph))
        })
    }

    #[test]
    fn glyphs_are_rectangular() {
        for (name, glyph) in glyphs() {
            let width = glyph[0].chars().count();
            for (row, line) in glyph.iter().enumerate() {
                assert_eq!(
                    line.chars().count(),
                    width,
                    "{name}, row {row} of {glyph:?} has a different width"
                );
            }
        }
    }

    #[test]
    fn every_glyph_of_a_font_is_the_same_height() {
        for font in &FONTS {
            let height = usize::from(font.height());
            assert!(height > 0, "{} has no rows", font.name);
            for (d, glyph) in font.digits.iter().enumerate() {
                assert_eq!(glyph.len(), height, "{}, digit {d}", font.name);
            }
            assert_eq!(font.colon.len(), height, "{}, colon", font.name);
        }
    }

    #[test]
    fn glyphs_only_use_blocks_and_space() {
        for (name, glyph) in glyphs() {
            for line in glyph {
                assert!(
                    line.chars().all(|c| matches!(c, '█' | '▒' | ' ')),
                    "{name}: bad row {line:?}"
                );
            }
        }
    }

    #[test]
    fn every_font_has_a_glyph_for_every_digit_and_the_colon() {
        for font in &FONTS {
            for c in "0123456789:".chars() {
                assert!(font.glyph(c).is_some(), "{} has no {c:?}", font.name);
            }
            assert!(font.glyph('x').is_none(), "{} invented an x", font.name);
        }
    }

    #[test]
    fn all_digits_of_a_font_share_one_width() {
        for font in &FONTS {
            let width = glyph_width(font.digits[0]);
            for (d, glyph) in font.digits.iter().enumerate() {
                assert_eq!(glyph_width(glyph), width, "{}, digit {d}", font.name);
            }
        }
    }

    #[test]
    fn names_are_sorted_and_unique() {
        let names = Font::all_names();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted, "the font list should be sorted and unique");
    }

    #[test]
    fn the_default_font_exists() {
        assert_eq!(Font::fallback().name(), DEFAULT);
        assert!(Font::by_name(DEFAULT).is_some());
        assert!(Font::by_name("nope").is_none());
    }

    #[test]
    fn text_width_counts_glyphs_and_gaps() {
        let block = Font::by_name("block").unwrap();
        // 6 digits + 2 colons + 7 gaps
        assert_eq!(block.text_width("12:34:56", 1), 6 * 6 + 2 * 2 + 7);
        // 4 digits + 1 colon + 4 gaps
        assert_eq!(block.text_width("12:34", 1), 4 * 6 + 2 + 4);
        assert_eq!(block.text_width("7", 1), 6);
        assert_eq!(block.text_width("", 1), 0);
    }

    #[test]
    fn scaling_multiplies_both_axes_by_the_same_amount() {
        for font in &FONTS {
            let one = font.text_width("12:34:56", 1);
            let height = font.height();
            for scale in 1..=MAX_SCALE {
                assert_eq!(
                    font.text_width("12:34:56", scale),
                    one * u16::from(scale),
                    "{} at {scale}x",
                    font.name
                );
                assert_eq!(font.scaled_height(scale), height * u16::from(scale));
            }
        }
    }

    #[test]
    fn slim_is_the_narrowest_font() {
        let widest = Font::all_names()
            .into_iter()
            .filter_map(Font::by_name)
            .map(|font| font.text_width("12:34:56", 1))
            .max();
        let slim = Font::by_name("slim").unwrap().text_width("12:34:56", 1);
        assert!(
            Some(slim) < widest,
            "slim is meant to be the one that fits a narrow terminal"
        );
    }

    #[test]
    fn scale_is_clamped_to_something_drawable() {
        assert_eq!(clamp_scale(0), 1);
        assert_eq!(clamp_scale(1), 1);
        assert_eq!(clamp_scale(MAX_SCALE), MAX_SCALE);
        assert_eq!(clamp_scale(200), MAX_SCALE);
    }
}
