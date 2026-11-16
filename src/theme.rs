//! Colour themes for the clock and for everything the animations draw.
//!
//! A theme is a brightness ramp. Instead of handing every animation a set of
//! colours to use, the theme re-colours what they drew: each pixel keeps the
//! brightness the animation chose and takes its hue from the ramp. Fire still
//! rises and matrix still fades down its trails, but in the theme's colours,
//! and no animation needs to know a theme exists.
//!
//! The `default` theme has no ramp at all, so it costs nothing and leaves
//! each animation's own palette alone.

use ratatui::{buffer::Buffer, layout::Rect, style::Color};

use crate::render::{Rgb, luma, ramp, rgb};

/// Theme used when `--theme` is not given.
pub const DEFAULT: &str = "default";

/// Accent colours the bouncing clock cycles through.
///
/// Their brightnesses are spread out on purpose: a theme maps colours by
/// brightness, so these stay apart in every theme.
const ACCENTS: [Rgb; 6] = [
    (255, 255, 255),
    (120, 200, 255),
    (255, 170, 60),
    (150, 240, 160),
    (240, 120, 180),
    (90, 120, 200),
];

/// A colour theme.
#[derive(Debug, Clone)]
pub struct Theme {
    name: &'static str,
    /// Colour of the clock digits.
    clock: Rgb,
    /// Colour of the date line.
    date: Rgb,
    /// Brightness to colour table for animation output, or `None` to leave
    /// animation colours untouched.
    ramp: Option<[Rgb; 256]>,
}

impl Theme {
    /// Every theme name, in the order they are offered to the user.
    pub const fn all_names() -> &'static [&'static str] {
        &["amber", "default", "gruvbox", "mono", "nord"]
    }

    /// Build the theme called `name`, or `None` if there is no such theme.
    pub fn by_name(name: &str) -> Option<Self> {
        let (clock, date, stops): (Rgb, Rgb, &[Rgb]) = match name {
            DEFAULT => {
                return Some(Self {
                    name: DEFAULT,
                    clock: (255, 255, 255),
                    date: (170, 170, 170),
                    ramp: None,
                });
            }
            "mono" => (
                (255, 255, 255),
                (150, 150, 150),
                &[(0, 0, 0), (255, 255, 255)],
            ),
            "amber" => (
                (255, 183, 61),
                (188, 118, 28),
                &[(0, 0, 0), (92, 40, 0), (255, 160, 20), (255, 232, 164)],
            ),
            "nord" => (
                (236, 239, 244),
                (136, 192, 208),
                &[
                    (46, 52, 64),
                    (76, 86, 106),
                    (94, 129, 172),
                    (136, 192, 208),
                    (236, 239, 244),
                ],
            ),
            "gruvbox" => (
                (251, 241, 199),
                (250, 189, 47),
                &[
                    (40, 40, 40),
                    (157, 62, 12),
                    (214, 93, 14),
                    (250, 189, 47),
                    (235, 219, 178),
                ],
            ),
            _ => return None,
        };

        // `by_name` is given a &'static str from the registry, but a caller
        // could pass anything; look the canonical name back up so the stored
        // name always has the 'static lifetime the struct needs.
        let name = Self::all_names().iter().find(|known| **known == name)?;
        Some(Self {
            name,
            clock,
            date,
            ramp: Some(ramp(stops)),
        })
    }

    /// This theme's name.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Colour for the clock digits.
    pub fn clock_color(&self) -> Color {
        rgb(self.clock)
    }

    /// Colour for the date line.
    pub fn date_color(&self) -> Color {
        rgb(self.date)
    }

    /// One of the accent colours, taken through the theme's ramp.
    ///
    /// `index` wraps, so a bounce counter can be passed straight in.
    pub fn accent(&self, index: usize) -> Color {
        let accent = ACCENTS[index % ACCENTS.len()];
        match &self.ramp {
            Some(ramp) => rgb(ramp[usize::from(luma(accent))]),
            None => rgb(accent),
        }
    }

    /// Re-colour everything drawn inside `rect`.
    ///
    /// Only [`Color::Rgb`] cells are touched: the animations use nothing else,
    /// while the black background and the clock's own colours are named and
    /// stay as they are.
    pub fn apply(&self, buf: &mut Buffer, rect: Rect) {
        let Some(ramp) = &self.ramp else {
            return; // the default theme is the identity
        };

        for y in rect.top()..rect.bottom() {
            for x in rect.left()..rect.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    let fg = remap(cell.fg, ramp);
                    let bg = remap(cell.bg, ramp);
                    cell.set_fg(fg).set_bg(bg);
                }
            }
        }
    }
}

/// One colour taken through a ramp, keeping its brightness.
fn remap(color: Color, ramp: &[Rgb; 256]) -> Color {
    match color {
        Color::Rgb(r, g, b) => rgb(ramp[usize::from(luma((r, g, b)))]),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_theme_can_be_built() {
        for name in Theme::all_names() {
            let theme = Theme::by_name(name).unwrap_or_else(|| panic!("{name} is missing"));
            assert_eq!(&theme.name(), name, "name() must match the registry key");
        }
    }

    #[test]
    fn unknown_themes_are_rejected() {
        assert!(Theme::by_name("definitely-not-a-theme").is_none());
    }

    #[test]
    fn names_are_sorted_and_unique() {
        let names = Theme::all_names();
        let mut sorted = names.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted, names,
            "keep all_names sorted and free of duplicates"
        );
    }

    #[test]
    fn the_default_theme_is_in_the_list() {
        assert!(Theme::all_names().contains(&DEFAULT));
    }

    #[test]
    fn the_default_theme_leaves_colours_alone() {
        let theme = Theme::by_name(DEFAULT).unwrap();
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_fg(Color::Rgb(200, 30, 10));

        theme.apply(&mut buf, area);
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(200, 30, 10));
    }

    #[test]
    fn a_theme_recolours_but_keeps_brightness_order() {
        let theme = Theme::by_name("nord").unwrap();
        let area = Rect::new(0, 0, 3, 1);
        let mut buf = Buffer::empty(area);
        // Three reds of rising brightness; fire would draw something like it.
        buf[(0, 0)].set_fg(Color::Rgb(40, 0, 0));
        buf[(1, 0)].set_fg(Color::Rgb(140, 0, 0));
        buf[(2, 0)].set_fg(Color::Rgb(255, 0, 0));

        theme.apply(&mut buf, area);

        let brightness = |x: u16| match buf[(x, 0)].fg {
            Color::Rgb(r, g, b) => luma((r, g, b)),
            other => panic!("expected an Rgb colour, got {other:?}"),
        };
        assert!(brightness(0) < brightness(1), "ordering lost");
        assert!(brightness(1) < brightness(2), "ordering lost");
        assert_ne!(buf[(2, 0)].fg, Color::Rgb(255, 0, 0), "nothing was themed");
    }

    #[test]
    fn named_colours_pass_straight_through() {
        let theme = Theme::by_name("mono").unwrap();
        let area = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_fg(Color::White).set_bg(Color::Black);

        theme.apply(&mut buf, area);
        assert_eq!(buf[(0, 0)].fg, Color::White);
        assert_eq!(buf[(0, 0)].bg, Color::Black);
    }

    #[test]
    fn mono_turns_everything_grey() {
        let theme = Theme::by_name("mono").unwrap();
        let area = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_fg(Color::Rgb(20, 200, 90));

        theme.apply(&mut buf, area);
        let Color::Rgb(r, g, b) = buf[(0, 0)].fg else {
            panic!("expected an Rgb colour");
        };
        assert_eq!((r, g), (g, b), "a grey has equal components");
    }

    #[test]
    fn accents_wrap_and_stay_distinct() {
        for name in Theme::all_names() {
            let theme = Theme::by_name(name).unwrap();
            assert_eq!(theme.accent(0), theme.accent(ACCENTS.len()), "{name}");
            assert_ne!(theme.accent(0), theme.accent(1), "{name} accents collided");
        }
    }

    #[test]
    fn apply_only_touches_the_given_rect() {
        let theme = Theme::by_name("amber").unwrap();
        let mut buf = Buffer::empty(Rect::new(0, 0, 4, 2));
        buf[(0, 0)].set_fg(Color::Rgb(10, 200, 10));
        buf[(3, 1)].set_fg(Color::Rgb(10, 200, 10));

        theme.apply(&mut buf, Rect::new(0, 0, 2, 1));

        assert_ne!(buf[(0, 0)].fg, Color::Rgb(10, 200, 10), "inside not themed");
        assert_eq!(
            buf[(3, 1)].fg,
            Color::Rgb(10, 200, 10),
            "outside was themed"
        );
    }
}
