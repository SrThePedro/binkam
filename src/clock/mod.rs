//! The big centred clock and the date line underneath it.

pub mod font;

use std::fmt::Write as _;

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::render::text_cols;
use font::{GLYPH_GAP, GLYPH_HEIGHT};

/// Turkish month names, indexed by [`Datelike::month0`].
const MONTHS: [&str; 12] = [
    "Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim",
    "Kasım", "Aralık",
];

/// Turkish weekday names, indexed by `Weekday::num_days_from_monday`.
const WEEKDAYS: [&str; 7] = [
    "Pazartesi",
    "Salı",
    "Çarşamba",
    "Perşembe",
    "Cuma",
    "Cumartesi",
    "Pazar",
];

/// Blank rows between the clock and the date line.
const DATE_GAP: u16 = 1;

/// Columns taken by the `HH:MM` fallback.
const SHORT_LEN: u16 = 5;

/// Rows taken by clock, gap and date line together.
const FULL_HEIGHT: u16 = GLYPH_HEIGHT + DATE_GAP + 1;

/// Where the clock ends up inside a given area, decided once and then used
/// both for drawing and for darkening the background behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// Block font at `clock`, with the date line at `date` when it fits.
    Big { clock: Rect, date: Option<Rect> },
    /// Plain text fallback; `seconds` is false once only `HH:MM` fits.
    Small { rect: Rect, seconds: bool },
    /// Not even `HH:MM` fits.
    Hidden,
}

impl Placement {
    /// The smallest rectangle covering everything this placement draws.
    fn bounds(self) -> Option<Rect> {
        match self {
            Self::Big { clock, date } => Some(match date {
                Some(date) => clock.union(date),
                None => clock,
            }),
            Self::Small { rect, .. } => Some(rect),
            Self::Hidden => None,
        }
    }
}

/// The colon is lit during the first half of every second.
const BLINK_HALF_NANOS: u32 = 500_000_000;

/// A clock drawn with block characters, with a Turkish date line below it.
///
/// Both strings are rebuilt in [`ClockWidget::update_time`], never while
/// drawing, so rendering a frame allocates nothing.
#[derive(Debug)]
pub struct ClockWidget {
    /// Always `HH:MM:SS`, plain ASCII.
    time_text: String,
    /// For example `15 Kasım 2026, Pazar`.
    date_text: String,
    /// The day `date_text` was built for, so it is only rebuilt at midnight.
    date_shown: Option<NaiveDate>,
    /// Whether the blinking colon is currently lit.
    colon_visible: bool,
    /// Colour of the digits.
    pub digit_color: Color,
    /// Colour of the date line.
    pub date_color: Color,
}

impl Default for ClockWidget {
    fn default() -> Self {
        Self {
            time_text: String::with_capacity(8),
            date_text: String::with_capacity(40),
            date_shown: None,
            colon_visible: true,
            digit_color: Color::White,
            date_color: Color::Gray,
        }
    }
}

impl ClockWidget {
    /// A clock that has not been given a time yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Point the clock at `now`, rebuilding the cached strings as needed.
    pub fn update_time(&mut self, now: NaiveDateTime) {
        self.colon_visible = now.nanosecond() < BLINK_HALF_NANOS;

        // `clear` keeps the already allocated capacity, so this only allocates
        // on the very first call.
        self.time_text.clear();
        // Writing into a String cannot fail; the Result is there for the trait.
        let _ = write!(
            self.time_text,
            "{:02}:{:02}:{:02}",
            now.hour(),
            now.minute(),
            now.second()
        );

        let date = now.date();
        if self.date_shown != Some(date) {
            self.date_shown = Some(date);
            self.date_text.clear();
            let _ = write!(
                self.date_text,
                "{} {} {}, {}",
                date.day(),
                MONTHS[date.month0() as usize],
                date.year(),
                WEEKDAYS[date.weekday().num_days_from_monday() as usize],
            );
        }
    }

    /// The current time as `HH:MM:SS`.
    pub fn time_text(&self) -> &str {
        &self.time_text
    }

    /// The current date line.
    pub fn date_text(&self) -> &str {
        &self.date_text
    }

    /// Smallest area the big clock needs, date line included.
    pub fn preferred_size(&self) -> (u16, u16) {
        let width = font::text_width(&self.time_text).max(text_cols(&self.date_text));
        (width, FULL_HEIGHT)
    }

    /// Draw the block digits with their top-left corner at `(x, y)`.
    fn render_big(&self, x: u16, y: u16, area: Rect, buf: &mut Buffer) {
        let style = Style::new().fg(self.digit_color);
        let mut cursor = x;

        for ch in self.time_text.chars() {
            let Some(glyph) = font::glyph(ch) else {
                continue;
            };

            // The colon keeps its width while dark, so the digits never shift.
            if ch != ':' || self.colon_visible {
                for (row, line) in glyph.iter().enumerate() {
                    // row < GLYPH_HEIGHT and col < 6, so these casts are exact.
                    let py = y.saturating_add(row as u16);
                    if py >= area.bottom() {
                        break;
                    }
                    for (col, pixel) in line.chars().enumerate() {
                        if pixel == ' ' {
                            continue; // transparent: leave the background alone
                        }
                        let px = cursor.saturating_add(col as u16);
                        if px >= area.right() {
                            break;
                        }
                        if let Some(cell) = buf.cell_mut((px, py)) {
                            cell.set_char(pixel).set_style(style);
                        }
                    }
                }
            }

            cursor = cursor.saturating_add(font::glyph_width(glyph).saturating_add(GLYPH_GAP));
        }
    }

    /// Work out where the clock goes, without drawing anything.
    fn placement(&self, area: Rect) -> Placement {
        if area.is_empty() {
            return Placement::Hidden;
        }

        let clock_width = font::text_width(&self.time_text);
        let date_width = text_cols(&self.date_text);

        if clock_width > 0 && area.width >= clock_width && area.height >= GLYPH_HEIGHT {
            let with_date =
                date_width > 0 && area.height >= FULL_HEIGHT && area.width >= date_width;
            let height = if with_date { FULL_HEIGHT } else { GLYPH_HEIGHT };
            let top = area.y + (area.height - height) / 2;

            return Placement::Big {
                clock: Rect::new(
                    area.x + (area.width - clock_width) / 2,
                    top,
                    clock_width,
                    GLYPH_HEIGHT,
                ),
                date: with_date.then(|| {
                    Rect::new(
                        area.x + (area.width - date_width) / 2,
                        top + GLYPH_HEIGHT + DATE_GAP,
                        date_width,
                        1,
                    )
                }),
            };
        }

        // Too small for the block font: drop the seconds, then give up.
        let full_width = text_cols(&self.time_text);
        let (width, seconds) = if area.width >= full_width {
            (full_width, true)
        } else {
            (SHORT_LEN, false)
        };
        if width == 0 || area.width < width {
            return Placement::Hidden;
        }

        Placement::Small {
            rect: Rect::new(
                area.x + (area.width - width) / 2,
                area.y + (area.height - 1) / 2,
                width,
                1,
            ),
            seconds,
        }
    }

    /// The area the clock would cover inside `area`, or `None` when it hides.
    ///
    /// The app uses this to darken just enough of the animation behind it.
    pub fn occupied_area(&self, area: Rect) -> Option<Rect> {
        self.placement(area).bounds()
    }
}

impl Widget for &ClockWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.placement(area) {
            Placement::Big { clock, date } => {
                self.render_big(clock.x, clock.y, area, buf);
                if let Some(date) = date {
                    buf.set_string(
                        date.x,
                        date.y,
                        &self.date_text,
                        Style::new().fg(self.date_color),
                    );
                }
            }
            Placement::Small { rect, seconds } => {
                let full = self.time_text.as_str();
                // `time_text` is ASCII, so byte slicing is column slicing.
                let text = if seconds {
                    full
                } else {
                    full.get(..usize::from(SHORT_LEN)).unwrap_or(full)
                };
                buf.set_string(rect.x, rect.y, text, Style::new().fg(self.digit_color));
            }
            Placement::Hidden => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32, nanos: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_nano_opt(hh, mm, ss, nanos)
            .unwrap()
    }

    fn clock_at(now: NaiveDateTime) -> ClockWidget {
        let mut clock = ClockWidget::new();
        clock.update_time(now);
        clock
    }

    /// Render into a bare buffer and return its rows as strings.
    fn rows(clock: &ClockWidget, width: u16, height: u16) -> Vec<String> {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        clock.render(area, &mut buf);
        (0..height)
            .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect())
            .collect()
    }

    #[test]
    fn formats_the_turkish_date() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));
        assert_eq!(clock.date_text(), "15 Kasım 2026, Pazar");
        assert_eq!(clock.time_text(), "12:34:56");
    }

    #[test]
    fn names_every_weekday_in_turkish() {
        // 2026-11-16 is a Monday.
        let expected = [
            "Pazartesi",
            "Salı",
            "Çarşamba",
            "Perşembe",
            "Cuma",
            "Cumartesi",
            "Pazar",
        ];
        for (offset, name) in expected.iter().enumerate() {
            let day = 16 + u32::try_from(offset).unwrap();
            let clock = clock_at(at(2026, 11, day, 0, 0, 0, 0));
            assert!(
                clock.date_text().ends_with(name),
                "{} should end with {name}",
                clock.date_text()
            );
        }
    }

    #[test]
    fn pads_single_digit_times_and_days() {
        let clock = clock_at(at(2026, 1, 3, 9, 5, 7, 0));
        assert_eq!(clock.time_text(), "09:05:07");
        assert_eq!(clock.date_text(), "3 Ocak 2026, Cumartesi");
    }

    #[test]
    fn date_line_survives_a_time_only_change() {
        let mut clock = clock_at(at(2026, 11, 15, 12, 0, 0, 0));
        clock.update_time(at(2026, 11, 15, 12, 0, 1, 0));
        assert_eq!(clock.date_text(), "15 Kasım 2026, Pazar");
    }

    #[test]
    fn date_line_follows_midnight() {
        let mut clock = clock_at(at(2026, 11, 15, 23, 59, 59, 0));
        clock.update_time(at(2026, 11, 16, 0, 0, 0, 0));
        assert_eq!(clock.date_text(), "16 Kasım 2026, Pazartesi");
    }

    #[test]
    fn colon_blinks_once_per_second() {
        assert!(clock_at(at(2026, 11, 15, 12, 0, 0, 0)).colon_visible);
        assert!(clock_at(at(2026, 11, 15, 12, 0, 0, 499_000_000)).colon_visible);
        assert!(!clock_at(at(2026, 11, 15, 12, 0, 0, 500_000_000)).colon_visible);
        assert!(!clock_at(at(2026, 11, 15, 12, 0, 0, 999_000_000)).colon_visible);
    }

    #[test]
    fn dark_colon_does_not_move_the_digits() {
        let lit = rows(&clock_at(at(2026, 11, 15, 12, 34, 56, 0)), 80, 24);
        let dark = rows(&clock_at(at(2026, 11, 15, 12, 34, 56, 700_000_000)), 80, 24);

        let first_block = |rs: &[String]| {
            rs.iter()
                .find_map(|r| r.find('█'))
                .expect("a block should be drawn")
        };
        assert_eq!(first_block(&lit), first_block(&dark));
        assert!(
            lit.join("").matches('█').count() > dark.join("").matches('█').count(),
            "the lit colon should add blocks"
        );
    }

    #[test]
    fn draws_the_big_clock_and_the_date_on_a_normal_terminal() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));
        let rows = rows(&clock, 80, 24);

        assert!(rows.iter().any(|r| r.contains('█')), "no digits drawn");
        assert!(
            rows.iter().any(|r| r.contains("15 Kasım 2026, Pazar")),
            "no date line drawn"
        );

        // The block is vertically centred: rows 0 and 23 stay untouched.
        assert!(rows[0].trim().is_empty());
        assert!(rows[23].trim().is_empty());
    }

    #[test]
    fn hides_the_date_when_the_terminal_is_short() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));
        let rows = rows(&clock, 80, 5);
        assert!(rows.iter().any(|r| r.contains('█')));
        assert!(!rows.iter().any(|r| r.contains("Kasım")));
    }

    #[test]
    fn falls_back_to_small_text_when_the_font_does_not_fit() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));

        let wide_enough = rows(&clock, 20, 5).join("\n");
        assert!(wide_enough.contains("12:34:56"), "{wide_enough}");

        let narrow = rows(&clock, 6, 3).join("\n");
        assert!(narrow.contains("12:34"), "{narrow}");
        assert!(!narrow.contains("12:34:56"));
    }

    #[test]
    fn tiny_areas_do_not_panic() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));
        for (w, h) in [(0, 0), (1, 1), (3, 1), (5, 1), (4, 2), (47, 4), (46, 7)] {
            let _ = rows(&clock, w, h);
        }
    }

    #[test]
    fn preferred_size_covers_clock_and_date() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));
        assert_eq!(clock.preferred_size(), (47, 7));
    }
}
