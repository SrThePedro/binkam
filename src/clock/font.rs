//! A block-character font for the big clock.
//!
//! Every glyph is a fixed number of rows made of `█` (lit) and spaces
//! (transparent). Spaces are never drawn, so whatever is behind the clock —
//! an animation, from phase 2 on — keeps showing through.

/// Rows in every glyph.
pub const GLYPH_HEIGHT: u16 = 5;

/// Blank columns inserted between two neighbouring glyphs.
pub const GLYPH_GAP: u16 = 1;

/// A single glyph: one string per row, all of them the same length.
pub type Glyph = [&'static str; GLYPH_HEIGHT as usize];

/// Seven-segment style digits, six columns wide.
const DIGITS: [Glyph; 10] = [
    ["██████", "██  ██", "██  ██", "██  ██", "██████"], // 0
    ["    ██", "    ██", "    ██", "    ██", "    ██"], // 1
    ["██████", "    ██", "██████", "██    ", "██████"], // 2
    ["██████", "    ██", "██████", "    ██", "██████"], // 3
    ["██  ██", "██  ██", "██████", "    ██", "    ██"], // 4
    ["██████", "██    ", "██████", "    ██", "██████"], // 5
    ["██████", "██    ", "██████", "██  ██", "██████"], // 6
    ["██████", "    ██", "    ██", "    ██", "    ██"], // 7
    ["██████", "██  ██", "██████", "██  ██", "██████"], // 8
    ["██████", "██  ██", "██████", "    ██", "██████"], // 9
];

/// The separator between hours, minutes and seconds.
const COLON: Glyph = ["  ", "██", "  ", "██", "  "];

/// The glyph for `c`, or `None` if the font has no such character.
pub fn glyph(c: char) -> Option<&'static Glyph> {
    match c {
        ':' => Some(&COLON),
        // `to_digit` keeps us from hand-rolling the ASCII arithmetic.
        _ => c.to_digit(10).map(|d| &DIGITS[d as usize]),
    }
}

/// Column count of a glyph, taken from its first row.
pub fn glyph_width(glyph: &Glyph) -> u16 {
    // Every row of a glyph has the same length; `glyphs_are_rectangular` guards that.
    u16::try_from(glyph[0].chars().count()).unwrap_or(u16::MAX)
}

/// Total width of `text` once rendered, gaps between glyphs included.
///
/// Characters the font does not know are skipped.
pub fn text_width(text: &str) -> u16 {
    let mut width = 0u16;
    let mut drawn = 0u16;
    for glyph in text.chars().filter_map(glyph) {
        width = width.saturating_add(glyph_width(glyph));
        drawn = drawn.saturating_add(1);
    }
    // n glyphs have n-1 gaps between them.
    width.saturating_add(drawn.saturating_sub(1).saturating_mul(GLYPH_GAP))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyphs_are_rectangular() {
        for (d, glyph) in DIGITS.iter().enumerate() {
            let width = glyph[0].chars().count();
            for (row, line) in glyph.iter().enumerate() {
                assert_eq!(
                    line.chars().count(),
                    width,
                    "digit {d}, row {row} has a different width"
                );
            }
        }
        for line in &COLON {
            assert_eq!(line.chars().count(), COLON[0].chars().count());
        }
    }

    #[test]
    fn glyphs_only_use_block_and_space() {
        for glyph in DIGITS.iter().chain([&COLON]) {
            for line in glyph {
                assert!(
                    line.chars().all(|c| c == '█' || c == ' '),
                    "bad row {line:?}"
                );
            }
        }
    }

    #[test]
    fn digits_are_six_columns_wide() {
        for glyph in &DIGITS {
            assert_eq!(glyph_width(glyph), 6);
        }
        assert_eq!(glyph_width(&COLON), 2);
    }

    #[test]
    fn lookup_matches_characters() {
        assert_eq!(glyph('7'), Some(&DIGITS[7]));
        assert_eq!(glyph(':'), Some(&COLON));
        assert!(glyph('x').is_none());
    }

    #[test]
    fn text_width_counts_glyphs_and_gaps() {
        // 6 digits + 2 colons + 7 gaps
        assert_eq!(text_width("12:34:56"), 6 * 6 + 2 * 2 + 7);
        // 4 digits + 1 colon + 4 gaps
        assert_eq!(text_width("12:34"), 4 * 6 + 2 + 4);
        assert_eq!(text_width("7"), 6);
        assert_eq!(text_width(""), 0);
    }
}
