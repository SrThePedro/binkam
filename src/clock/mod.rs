//! The big centred clock and the date line underneath it.

pub mod font;
pub mod motion;

use std::fmt::Write as _;

use chrono::{Datelike, NaiveDate, NaiveDateTime, Timelike};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use crate::render::text_cols;
use font::{Font, GLYPH_GAP};

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

/// Marker added to the date line when the hours run 1 to 12.
const BEFORE_NOON: &str = "ÖÖ";
const AFTER_NOON: &str = "ÖS";

/// Blank rows between the clock and the date line.
const DATE_GAP: u16 = 1;

/// Columns taken by `HH:MM`, the time without its seconds.
const SHORT_LEN: u16 = 5;

/// Whether the hours run 0 to 23 or 1 to 12.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HourFormat {
    /// 1 to 12, with a marker on the date line.
    H12,
    /// 0 to 23.
    #[default]
    H24,
}

impl HourFormat {
    /// The format written as the number of hours on the dial.
    pub fn hours(self) -> u8 {
        match self {
            Self::H12 => 12,
            Self::H24 => 24,
        }
    }

    /// The format for a dial of `hours`, or `None` for anything but 12 or 24.
    pub fn from_hours(hours: u8) -> Option<Self> {
        match hours {
            12 => Some(Self::H12),
            24 => Some(Self::H24),
            _ => None,
        }
    }

    /// The hour to display for a 0 to 23 wall-clock hour.
    fn hour(self, hour24: u32) -> u32 {
        match self {
            Self::H24 => hour24,
            // Midnight and noon are both twelve, not zero.
            Self::H12 => match hour24 % 12 {
                0 => 12,
                hour => hour,
            },
        }
    }
}

/// Where the clock ends up inside a given area, decided once and then used
/// both for drawing and for darkening the background behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// Block font at `clock`, with the date line at `date` when it fits.
    /// `seconds` is false once the seconds had to go to make it fit.
    Big {
        clock: Rect,
        date: Option<Rect>,
        seconds: bool,
        /// Size multiplier the glyphs ended up being drawn at, which is the
        /// one that was asked for unless it had to shrink to fit.
        scale: u8,
    },
    /// Plain text fallback; `seconds` is false once only `HH:MM` fits.
    Small { rect: Rect, seconds: bool },
    /// Not even `HH:MM` fits.
    Hidden,
}

impl Placement {
    /// The smallest rectangle covering everything this placement draws.
    fn bounds(self) -> Option<Rect> {
        match self {
            Self::Big { clock, date, .. } => Some(match date {
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
    /// What `date_text` was built for: the day, and whether it was afternoon.
    ///
    /// The afternoon flag is part of the key because a 12-hour clock puts a
    /// marker on the date line, and that changes at noon, not at midnight.
    date_shown: Option<(NaiveDate, bool)>,
    /// Whether the blinking colon is currently lit.
    colon_visible: bool,
    /// Whether the time carries its seconds.
    show_seconds: bool,
    /// Whether the date line is drawn at all.
    show_date: bool,
    /// Whether the hours run 0 to 23 or 1 to 12.
    hour_format: HourFormat,
    /// The glyphs the big clock is drawn with.
    font: &'static Font,
    /// How many cells wide and tall one glyph pixel becomes.
    scale: u8,
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
            show_seconds: true,
            show_date: true,
            hour_format: HourFormat::default(),
            font: Font::fallback(),
            scale: 1,
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

    /// Choose what the clock shows.
    pub fn with_format(
        mut self,
        show_seconds: bool,
        show_date: bool,
        hour_format: HourFormat,
    ) -> Self {
        self.show_seconds = show_seconds;
        self.show_date = show_date;
        self.hour_format = hour_format;
        // Any cached date line was built for the old settings.
        self.date_shown = None;
        self
    }

    /// Choose the glyphs and how big to draw them.
    ///
    /// A size the terminal cannot fit is not an error: [`Self::placement`]
    /// works its way back down to one that fits.
    pub fn with_font(mut self, font: &'static Font, scale: u8) -> Self {
        self.font = font;
        self.scale = font::clamp_scale(scale);
        self
    }

    /// The glyphs currently in use.
    pub fn font(&self) -> &'static Font {
        self.font
    }

    /// The size multiplier currently asked for.
    pub fn scale(&self) -> u8 {
        self.scale
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
            "{:02}:{:02}",
            self.hour_format.hour(now.hour()),
            now.minute()
        );
        if self.show_seconds {
            let _ = write!(self.time_text, ":{:02}", now.second());
        }

        let afternoon = now.hour() >= 12;
        let date = now.date();
        if !self.show_date {
            self.date_text.clear();
            self.date_shown = None;
            return;
        }
        if self.date_shown != Some((date, afternoon)) {
            self.date_shown = Some((date, afternoon));
            self.date_text.clear();
            let _ = write!(
                self.date_text,
                "{} {} {}, {}",
                date.day(),
                MONTHS[date.month0() as usize],
                date.year(),
                WEEKDAYS[date.weekday().num_days_from_monday() as usize],
            );
            if self.hour_format == HourFormat::H12 {
                // The big font has only digits, so the only place to say
                // which half of the day it is, is the date line.
                let marker = if afternoon { AFTER_NOON } else { BEFORE_NOON };
                let _ = write!(self.date_text, " · {marker}");
            }
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
        let width = self
            .font
            .text_width(&self.time_text, self.scale)
            .max(text_cols(&self.date_text));
        (width, self.font.scaled_height(self.scale) + DATE_GAP + 1)
    }

    /// The time as shown, with or without its seconds.
    ///
    /// `time_text` is ASCII, so slicing it by bytes is slicing by columns.
    /// Asking for seconds that are switched off simply gets `HH:MM`.
    fn time_slice(&self, seconds: bool) -> &str {
        if seconds {
            &self.time_text
        } else {
            self.time_text
                .get(..usize::from(SHORT_LEN))
                .unwrap_or(&self.time_text)
        }
    }

    /// Draw the block digits with their top-left corner at `(x, y)`.
    ///
    /// At `scale` 1 one glyph pixel is one cell; above that it becomes a
    /// `scale` by `scale` square of them.
    fn render_big(&self, text: &str, x: u16, y: u16, scale: u8, area: Rect, buf: &mut Buffer) {
        let style = Style::new().fg(self.digit_color);
        let scale = u16::from(scale);
        let mut cursor = x;

        for ch in text.chars() {
            let Some(glyph) = self.font.glyph(ch) else {
                continue;
            };

            // The colon keeps its width while dark, so the digits never shift.
            if ch != ':' || self.colon_visible {
                for (row, line) in glyph.iter().enumerate() {
                    // A glyph is a handful of rows and columns, so these casts
                    // are exact.
                    let top = y.saturating_add((row as u16).saturating_mul(scale));
                    if top >= area.bottom() {
                        break;
                    }
                    for (col, pixel) in line.chars().enumerate() {
                        if pixel == ' ' {
                            continue; // transparent: leave the background alone
                        }
                        let left = cursor.saturating_add((col as u16).saturating_mul(scale));
                        if left >= area.right() {
                            break;
                        }
                        for py in top..top.saturating_add(scale).min(area.bottom()) {
                            for px in left..left.saturating_add(scale).min(area.right()) {
                                if let Some(cell) = buf.cell_mut((px, py)) {
                                    cell.set_char(pixel).set_style(style);
                                }
                            }
                        }
                    }
                }
            }

            cursor = cursor.saturating_add(
                font::glyph_width(glyph)
                    .saturating_add(GLYPH_GAP)
                    .saturating_mul(scale),
            );
        }
    }

    /// Work out where the clock goes, without drawing anything.
    fn placement(&self, area: Rect) -> Placement {
        if area.is_empty() {
            return Placement::Hidden;
        }

        let date_width = text_cols(&self.date_text);

        // Biggest first: the size that was asked for, then that size without
        // the seconds, then one size smaller, and so on. An explicit size is
        // a deliberate choice, so the seconds are given up before it is.
        for scale in (1..=font::clamp_scale(self.scale)).rev() {
            let clock_height = self.font.scaled_height(scale);
            if area.height < clock_height {
                continue;
            }
            for seconds in [true, false] {
                let clock_width = self.font.text_width(self.time_slice(seconds), scale);
                if clock_width == 0 || area.width < clock_width {
                    continue;
                }

                let full_height = clock_height.saturating_add(DATE_GAP + 1);
                let with_date =
                    date_width > 0 && area.height >= full_height && area.width >= date_width;
                let height = if with_date { full_height } else { clock_height };
                let top = area.y + (area.height - height) / 2;

                return Placement::Big {
                    clock: Rect::new(
                        area.x + (area.width - clock_width) / 2,
                        top,
                        clock_width,
                        clock_height,
                    ),
                    date: with_date.then(|| {
                        Rect::new(
                            area.x + (area.width - date_width) / 2,
                            top + clock_height + DATE_GAP,
                            date_width,
                            1,
                        )
                    }),
                    seconds,
                    scale,
                };
            }
        }

        // Too small for the block font: drop the seconds, then give up.
        let full_width = text_cols(self.time_slice(true));
        let (width, seconds) = if area.width >= full_width {
            (full_width, true)
        } else {
            (text_cols(self.time_slice(false)), false)
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

    /// The size the clock will take inside `area`, or `(0, 0)` when nothing
    /// fits.
    ///
    /// Handing this straight back as the clock's own area is stable: the
    /// widget makes the same big-or-small decision for a rectangle that is
    /// exactly its own size.
    pub fn measure(&self, area: Rect) -> (u16, u16) {
        match self.placement(area).bounds() {
            Some(rect) => (rect.width, rect.height),
            None => (0, 0),
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
            Placement::Big {
                clock,
                date,
                seconds,
                scale,
            } => {
                self.render_big(self.time_slice(seconds), clock.x, clock.y, scale, area, buf);
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
                let text = self.time_slice(seconds);
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
    fn hour_formats_round_trip_through_their_dial_size() {
        for format in [HourFormat::H12, HourFormat::H24] {
            assert_eq!(HourFormat::from_hours(format.hours()), Some(format));
        }
        assert_eq!(HourFormat::from_hours(13), None);
        assert_eq!(HourFormat::default(), HourFormat::H24);
    }

    #[test]
    fn seconds_can_be_switched_off() {
        let mut clock = ClockWidget::new().with_format(false, true, HourFormat::H24);
        clock.update_time(at(2026, 11, 15, 12, 34, 56, 0));
        assert_eq!(clock.time_text(), "12:34");
    }

    #[test]
    fn the_date_line_can_be_switched_off() {
        let mut clock = ClockWidget::new().with_format(true, false, HourFormat::H24);
        clock.update_time(at(2026, 11, 15, 12, 34, 56, 0));
        assert_eq!(clock.date_text(), "");

        let rows = rows(&clock, 80, 24);
        assert!(rows.iter().any(|r| r.contains('█')), "the clock went too");
        assert!(!rows.iter().any(|r| r.contains("Kasım")));
    }

    #[test]
    fn twelve_hour_format_renumbers_the_hours() {
        let twelve = |hour| {
            let mut clock = ClockWidget::new().with_format(true, true, HourFormat::H12);
            clock.update_time(at(2026, 11, 15, hour, 5, 0, 0));
            clock
        };

        assert_eq!(twelve(0).time_text(), "12:05:00", "midnight is twelve");
        assert_eq!(twelve(1).time_text(), "01:05:00");
        assert_eq!(twelve(12).time_text(), "12:05:00", "noon is twelve");
        assert_eq!(twelve(13).time_text(), "01:05:00");
        assert_eq!(twelve(23).time_text(), "11:05:00");
    }

    #[test]
    fn twelve_hour_format_marks_the_half_of_the_day() {
        let mut clock = ClockWidget::new().with_format(true, true, HourFormat::H12);
        clock.update_time(at(2026, 11, 15, 9, 0, 0, 0));
        assert_eq!(clock.date_text(), "15 Kasım 2026, Pazar · ÖÖ");

        // The marker has to follow noon, not just midnight: the cached date
        // line is keyed on the half of the day as well as the date.
        clock.update_time(at(2026, 11, 15, 15, 0, 0, 0));
        assert_eq!(clock.date_text(), "15 Kasım 2026, Pazar · ÖS");
    }

    #[test]
    fn a_24_hour_clock_has_no_marker() {
        let clock = clock_at(at(2026, 11, 15, 15, 0, 0, 0));
        assert_eq!(clock.date_text(), "15 Kasım 2026, Pazar");
    }

    #[test]
    fn the_block_font_drops_seconds_before_giving_up() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));
        // 47 columns for HH:MM:SS, 30 for HH:MM.
        assert_eq!(clock.font().text_width("12:34:56", 1), 47);
        assert_eq!(clock.font().text_width("12:34", 1), 30);

        let wide = rows(&clock, 47, 5).join("\n");
        assert!(wide.contains('█'));

        // Between the two widths the seconds go but the block font stays.
        let narrow = rows(&clock, 40, 5).join("\n");
        assert!(narrow.contains('█'), "fell back to plain text too early");
        assert_eq!(clock.measure(Rect::new(0, 0, 40, 5)), (30, 5));

        // Below that, plain text.
        let tiny = rows(&clock, 20, 5).join("\n");
        assert!(!tiny.contains('█'));
        assert!(tiny.contains("12:34:56"));
    }

    #[test]
    fn dropping_the_seconds_keeps_the_colon_blinking() {
        let dark = rows(&clock_at(at(2026, 11, 15, 12, 34, 56, 700_000_000)), 40, 5);
        let lit = rows(&clock_at(at(2026, 11, 15, 12, 34, 56, 0)), 40, 5);
        let count = |rs: &[String]| rs.join("").matches('█').count();
        assert!(count(&lit) > count(&dark));
    }

    #[test]
    fn measure_is_stable_when_fed_back_in() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));
        for area in [
            Rect::new(0, 0, 80, 24),
            Rect::new(0, 0, 47, 5),
            Rect::new(0, 0, 40, 7),
            Rect::new(0, 0, 30, 5),
            Rect::new(0, 0, 20, 5),
            Rect::new(0, 0, 6, 3),
        ] {
            let size = clock.measure(area);
            let own = Rect::new(0, 0, size.0, size.1);
            assert_eq!(clock.measure(own), size, "unstable for {area:?}");
        }
    }

    #[test]
    fn measure_gives_up_when_nothing_fits() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));
        assert_eq!(clock.measure(Rect::new(0, 0, 4, 1)), (0, 0));
        assert_eq!(clock.measure(Rect::ZERO), (0, 0));
    }

    #[test]
    fn preferred_size_covers_clock_and_date() {
        let clock = clock_at(at(2026, 11, 15, 12, 34, 56, 0));
        assert_eq!(clock.preferred_size(), (47, 7));
    }

    /// The same clock drawn with a named font at a given size.
    fn clock_with(name: &str, scale: u8) -> ClockWidget {
        let mut clock = ClockWidget::new().with_font(Font::by_name(name).unwrap(), scale);
        clock.update_time(at(2026, 11, 15, 12, 34, 56, 0));
        clock
    }

    #[test]
    fn every_font_draws_the_time_when_it_fits() {
        for name in Font::all_names() {
            let clock = clock_with(name, 1);
            let font = Font::by_name(name).unwrap();
            let width = font.text_width("12:34:56", 1);
            let text = rows(&clock, width, font.height() + 2).join("\n");
            assert!(text.contains('█'), "{name} drew no digits:\n{text}");
        }
    }

    #[test]
    fn a_bigger_size_takes_more_room() {
        let one = clock_with("block", 1);
        let two = clock_with("block", 2);
        let area = Rect::new(0, 0, 200, 60);
        let (narrow, short) = one.measure(area);
        let (wide, tall) = two.measure(area);
        assert_eq!(wide, narrow * 2, "the width should double");
        // The date line below it is one row either way, so the height grows
        // by the height of the glyphs alone.
        assert_eq!(tall - short, one.font().height());
    }

    #[test]
    fn an_oversized_clock_shrinks_until_it_fits() {
        // 4x block needs 188 columns; this terminal has 80.
        let clock = clock_with("block", 4);
        let area = Rect::new(0, 0, 80, 24);
        let (width, height) = clock.measure(area);
        assert!(width <= 80 && height <= 24, "{width}x{height} does not fit");
        let text = rows(&clock, 80, 24).join("\n");
        assert!(text.contains('█'), "gave up on the block font entirely");
    }

    #[test]
    fn the_size_asked_for_is_remembered_even_when_it_cannot_be_drawn() {
        let clock = clock_with("block", 4);
        assert_eq!(clock.scale(), 4);
        // Shrinking to fit is a drawing decision, not a change of settings.
        let _ = clock.measure(Rect::new(0, 0, 80, 24));
        assert_eq!(clock.scale(), 4);
    }

    #[test]
    fn an_impossible_size_is_clamped_rather_than_refused() {
        assert_eq!(clock_with("block", 0).scale(), 1);
        assert_eq!(clock_with("block", 250).scale(), font::MAX_SCALE);
    }

    #[test]
    fn slim_fits_a_terminal_that_block_cannot() {
        // 33 columns for slim against 47 for block.
        let area = Rect::new(0, 0, 36, 7);
        let slim = clock_with("slim", 1);
        let block = clock_with("block", 1);
        assert_eq!(
            slim.measure(area).0,
            33,
            "slim should still have room for the seconds"
        );
        assert_eq!(
            block.measure(area).0,
            30,
            "block should have dropped its seconds"
        );
        assert!(
            rows(&slim, area.width, area.height)
                .join("\n")
                .contains('█'),
            "slim drew no digits"
        );
    }

    #[test]
    fn every_font_and_size_survives_every_awkward_area() {
        for name in Font::all_names() {
            for scale in 1..=font::MAX_SCALE {
                let clock = clock_with(name, scale);
                for (width, height) in [(1, 1), (3, 2), (6, 5), (20, 5), (47, 5), (80, 24)] {
                    let lines = rows(&clock, width, height);
                    assert_eq!(lines.len(), usize::from(height), "{name} {scale}x");
                    assert!(
                        lines
                            .iter()
                            .all(|l| l.chars().count() == usize::from(width)),
                        "{name} at {scale}x drew a ragged frame on {width}x{height}"
                    );
                }
            }
        }
    }
}
