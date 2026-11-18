//! The settings mode: a key reminder along the bottom and one pop-up list.
//!
//! Everything that turns a setting into something showable, and a chosen
//! line back into a setting, lives here. The app only has to open a list,
//! move through it and hand the highlighted value back, which keeps the
//! knowledge of what a setting *is* in one place.

use std::fmt::Write as _;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
};

use crate::{
    anim,
    clock::{
        font::{self, Font},
        motion::ClockPosition,
    },
    color::ColorSetting,
    config::{Config, MAX_FPS},
    render::{self, text_cols},
    theme::Theme,
};

/// Frames per second worth offering. The one in force is added if it is not
/// already here, so a hand-written setting is never silently dropped.
const FPS_CHOICES: [u16; 5] = [15, 24, 30, 60, 120];

/// Minutes between animation changes worth offering.
const CYCLE_CHOICES: [u64; 6] = [0, 1, 3, 5, 10, 30];

/// Brightness left in the cells the pop-up covers; it has to be readable
/// over a busy animation.
const PANEL_KEEP: f32 = 0.06;

/// Fraction left behind the key reminder along the bottom.
const BAR_KEEP: f32 = 0.12;

/// Cells left clear between the pop-up and the edge of the screen.
const MARGIN: u16 = 1;

/// Something the settings mode can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Which background animation runs, or all of them in turn.
    Animation,
    /// Colour theme.
    Theme,
    /// Where the clock sits and how it moves.
    Position,
    /// Glyphs the big clock is drawn with.
    ClockFont,
    /// How big the big clock is drawn.
    ClockScale,
    /// Whether the time carries its seconds.
    Seconds,
    /// Whether the date line is drawn.
    Date,
    /// Whether the hours run 0 to 23 or 1 to 12.
    HourFormat,
    /// How much colour to use.
    Color,
    /// Whether the animation is darkened behind the clock.
    Backdrop,
    /// Minutes between automatic animation changes.
    Cycle,
    /// Frames per second.
    Fps,
}

impl Setting {
    /// Every setting, in the order the lists are offered.
    pub const ALL: [Self; 12] = [
        Self::Animation,
        Self::Theme,
        Self::Position,
        Self::ClockFont,
        Self::ClockScale,
        Self::Seconds,
        Self::Date,
        Self::HourFormat,
        Self::Color,
        Self::Backdrop,
        Self::Cycle,
        Self::Fps,
    ];

    /// The key that opens this setting's list.
    ///
    /// The letters are Turkish mnemonics, because that is the language the
    /// menu is written in: `k` for konum, `b` for boyut, `g` for gölge.
    pub fn key(self) -> char {
        match self {
            Self::Animation => 'a',
            Self::Theme => 't',
            Self::Position => 'k',
            Self::ClockFont => 'f',
            Self::ClockScale => 'b',
            Self::Seconds => 's',
            Self::Date => 'd',
            Self::HourFormat => 'h',
            Self::Color => 'r',
            Self::Backdrop => 'g',
            Self::Cycle => 'c',
            Self::Fps => 'z',
        }
    }

    /// The setting `key` opens, if it opens one.
    pub fn from_key(key: char) -> Option<Self> {
        Self::ALL.into_iter().find(|setting| setting.key() == key)
    }

    /// Heading of this setting's list.
    pub fn title(self) -> &'static str {
        match self {
            Self::Animation => "Animasyon",
            Self::Theme => "Tema",
            Self::Position => "Saat konumu",
            Self::ClockFont => "Saat fontu",
            Self::ClockScale => "Saat boyutu",
            Self::Seconds => "Saniye",
            Self::Date => "Tarih",
            Self::HourFormat => "Saat biçimi",
            Self::Color => "Renk",
            Self::Backdrop => "Saat gölgesi",
            Self::Cycle => "Animasyon değişimi",
            Self::Fps => "Kare hızı",
        }
    }

    /// Short name for the key reminder along the bottom.
    pub fn label(self) -> &'static str {
        match self {
            Self::Animation => "animasyon",
            Self::Theme => "tema",
            Self::Position => "konum",
            Self::ClockFont => "font",
            Self::ClockScale => "boyut",
            Self::Seconds => "saniye",
            Self::Date => "tarih",
            Self::HourFormat => "12/24",
            Self::Color => "renk",
            Self::Backdrop => "gölge",
            Self::Cycle => "değişim",
            Self::Fps => "hız",
        }
    }
}

/// One line of a list: what it reads as, and what it stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    label: String,
    value: String,
}

impl Item {
    fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }

    /// A line whose text is also the value it stands for.
    fn plain(value: &str) -> Self {
        Self::new(value, value)
    }
}

/// Both answers to a yes-or-no setting, with `on` first.
fn switch() -> Vec<Item> {
    vec![Item::new("açık", "true"), Item::new("kapalı", "false")]
}

/// Where `wanted` sits in `items`, or 0 when nothing matches.
///
/// Nothing matching is normal: `--anim fire --anim snow` pins two
/// animations, which is not one of the lines on offer.
fn index_of(items: &[Item], wanted: &str) -> usize {
    items
        .iter()
        .position(|item| item.value == wanted)
        .unwrap_or(0)
}

/// Every value `setting` can take, and which line is in force.
pub fn choices(setting: Setting, config: &Config) -> (Vec<Item>, usize) {
    match setting {
        Setting::Animation => {
            // An empty list of animations means every one of them in turn,
            // so that is a line of its own rather than a missing value.
            let mut items = vec![Item::new("tümü (sırayla)", "")];
            items.extend(anim::all_names().iter().map(|name| Item::plain(name)));
            let current = if config.anim.len() == 1 {
                config.anim[0].as_str()
            } else {
                ""
            };
            let index = index_of(&items, current);
            (items, index)
        }
        Setting::Theme => {
            let items: Vec<Item> = Theme::all_names().iter().map(|n| Item::plain(n)).collect();
            let index = index_of(&items, &config.theme);
            (items, index)
        }
        Setting::Position => {
            let items: Vec<Item> = ClockPosition::all_names()
                .iter()
                .map(|n| Item::plain(n))
                .collect();
            let index = index_of(&items, &config.clock_position);
            (items, index)
        }
        Setting::ClockFont => {
            let items: Vec<Item> = Font::all_names().into_iter().map(Item::plain).collect();
            let index = index_of(&items, &config.clock_font);
            (items, index)
        }
        Setting::ClockScale => {
            let items: Vec<Item> = (1..=font::MAX_SCALE)
                .map(|scale| Item::new(format!("{scale}x"), scale.to_string()))
                .collect();
            let index = index_of(&items, &config.clock_scale.to_string());
            (items, index)
        }
        Setting::Seconds => {
            let items = switch();
            let index = index_of(&items, &config.show_seconds.to_string());
            (items, index)
        }
        Setting::Date => {
            let items = switch();
            let index = index_of(&items, &config.show_date.to_string());
            (items, index)
        }
        Setting::Backdrop => {
            let items = switch();
            let index = index_of(&items, &config.clock_backdrop.to_string());
            (items, index)
        }
        Setting::HourFormat => {
            let items = vec![Item::new("24 saat", "24"), Item::new("12 saat", "12")];
            let index = index_of(&items, &config.hour_format.to_string());
            (items, index)
        }
        Setting::Color => {
            let items: Vec<Item> = ColorSetting::all_names()
                .iter()
                .map(|name| match *name {
                    "auto" => Item::new("auto (terminale sor)", "auto"),
                    "none" => Item::new("none (renksiz)", "none"),
                    other => Item::plain(other),
                })
                .collect();
            let index = index_of(&items, &config.color);
            (items, index)
        }
        Setting::Fps => {
            let mut values: Vec<u16> = FPS_CHOICES
                .into_iter()
                .filter(|fps| *fps <= MAX_FPS)
                .collect();
            keep_current(&mut values, config.fps);
            let items = values
                .iter()
                .map(|fps| Item::new(format!("{fps} fps"), fps.to_string()))
                .collect::<Vec<_>>();
            let index = index_of(&items, &config.fps.to_string());
            (items, index)
        }
        Setting::Cycle => {
            let mut values: Vec<u64> = CYCLE_CHOICES.to_vec();
            keep_current(&mut values, config.cycle_minutes);
            let items = values
                .iter()
                .map(|minutes| {
                    let label = if *minutes == 0 {
                        "kapalı".to_owned()
                    } else {
                        format!("{minutes} dakika")
                    };
                    Item::new(label, minutes.to_string())
                })
                .collect::<Vec<_>>();
            let index = index_of(&items, &config.cycle_minutes.to_string());
            (items, index)
        }
    }
}

/// Make sure the value in force is one of the lines on offer.
///
/// A number set by hand in the file, say `fps = 45`, is a real setting; it
/// should be visible in the list rather than quietly replaced by whichever
/// line happened to be first.
fn keep_current<T: Ord + Copy>(values: &mut Vec<T>, current: T) {
    if !values.contains(&current) {
        values.push(current);
        values.sort_unstable();
    }
}

/// Put `value`, taken from this setting's list, into `config`.
///
/// Values come from [`choices`], so a value that will not parse can only
/// mean a bug here; the setting is then left as it was.
pub fn apply(setting: Setting, value: &str, config: &mut Config) {
    match setting {
        Setting::Animation => {
            if value.is_empty() {
                config.anim.clear();
            } else {
                config.anim = vec![value.to_owned()];
            }
        }
        Setting::Theme => config.theme = value.to_owned(),
        Setting::Position => config.clock_position = value.to_owned(),
        Setting::ClockFont => config.clock_font = value.to_owned(),
        Setting::Color => config.color = value.to_owned(),
        Setting::ClockScale => {
            if let Ok(scale) = value.parse() {
                config.clock_scale = scale;
            }
        }
        Setting::Seconds => {
            if let Ok(on) = value.parse() {
                config.show_seconds = on;
            }
        }
        Setting::Date => {
            if let Ok(on) = value.parse() {
                config.show_date = on;
            }
        }
        Setting::Backdrop => {
            if let Ok(on) = value.parse() {
                config.clock_backdrop = on;
            }
        }
        Setting::HourFormat => {
            if let Ok(hours) = value.parse() {
                config.hour_format = hours;
            }
        }
        Setting::Fps => {
            if let Ok(fps) = value.parse() {
                config.fps = fps;
            }
        }
        Setting::Cycle => {
            if let Ok(minutes) = value.parse() {
                config.cycle_minutes = minutes;
            }
        }
    }
}

/// A pop-up list in the top left corner.
///
/// Every string it draws is built when the list opens, so drawing a frame
/// neither formats nor allocates.
#[derive(Debug)]
pub struct Menu {
    /// The setting being changed, or `None` for the key help, which changes
    /// nothing.
    setting: Option<Setting>,
    /// Rows, each already padded to the width inside the frame.
    rows: Vec<String>,
    /// What each row stands for; empty for the key help.
    values: Vec<String>,
    /// Top frame, heading included, and the bottom one.
    top: String,
    bottom: String,
    /// Columns inside the frame.
    inner: u16,
    /// Highlighted row.
    index: usize,
    /// The row the list opened on, which Esc goes back to.
    opened_on: usize,
    /// First row on screen, for a list too tall for the terminal.
    offset: usize,
}

impl Menu {
    /// A list for one setting, opened on the value in force.
    pub fn new(setting: Setting, items: &[Item], index: usize, area: Rect) -> Self {
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        let values = items.iter().map(|item| item.value.clone()).collect();
        Self::build(Some(setting), setting.title(), &labels, values, index, area)
    }

    /// The list of every key, which `?` opens.
    pub fn help(area: Rect) -> Self {
        let mut labels: Vec<String> = Setting::ALL
            .into_iter()
            .map(|setting| format!("{}   {}", setting.key(), setting.title()))
            .collect();
        labels.push(String::new());
        labels.push("n p sonraki / önceki animasyon".to_owned());
        labels.push("↑ ↓ listede gezin".to_owned());
        labels.push("⏎   seç ve kaydet".to_owned());
        labels.push("esc vazgeç".to_owned());
        labels.push("q   çık".to_owned());

        let borrowed: Vec<&str> = labels.iter().map(String::as_str).collect();
        Self::build(None, "Tuşlar", &borrowed, Vec::new(), 0, area)
    }

    /// Lay out a list: pad every row to one width and build the frame.
    fn build(
        setting: Option<Setting>,
        title: &str,
        labels: &[&str],
        values: Vec<String>,
        index: usize,
        area: Rect,
    ) -> Self {
        // "│ › label │": a marker column, a space either side of the text.
        let widest = labels
            .iter()
            .map(|label| text_cols(label))
            .max()
            .unwrap_or(0);
        let heading = format!("─ {title} ");
        let inner = (widest + 4).max(text_cols(&heading));

        let rows = labels
            .iter()
            .map(|label| {
                let mut row = String::from("   ");
                row.push_str(label);
                for _ in 0..inner.saturating_sub(text_cols(label) + 4) {
                    row.push(' ');
                }
                row.push(' ');
                row
            })
            .collect::<Vec<_>>();

        let dashes = usize::from(inner.saturating_sub(text_cols(&heading)));
        let mut menu = Self {
            setting,
            rows,
            values,
            top: format!("┌{heading}{}┐", "─".repeat(dashes)),
            bottom: format!("└{}┘", "─".repeat(usize::from(inner))),
            inner,
            index: 0,
            opened_on: index,
            offset: 0,
        };
        // Go through the mover, so a list that opens on a row below the fold
        // is scrolled to it.
        menu.move_to(index, area);
        menu
    }

    /// The setting this list changes, or `None` for the key help.
    pub fn setting(&self) -> Option<Setting> {
        self.setting
    }

    /// What the highlighted row stands for.
    pub fn selected_value(&self) -> Option<&str> {
        self.values.get(self.index).map(String::as_str)
    }

    /// What the list opened on, which Esc goes back to.
    pub fn opened_value(&self) -> Option<&str> {
        self.values.get(self.opened_on).map(String::as_str)
    }

    /// Move the highlight by `delta` rows, wrapping at both ends.
    pub fn move_by(&mut self, delta: isize, area: Rect) {
        let count = self.rows.len();
        if count == 0 {
            return;
        }
        let count = isize::try_from(count).unwrap_or(isize::MAX);
        let index = isize::try_from(self.index).unwrap_or(0);
        // `rem_euclid` wraps both ways, so -1 from the first row is the last.
        let wrapped = (index + delta).rem_euclid(count);
        self.move_to(usize::try_from(wrapped).unwrap_or(0), area);
    }

    /// Highlight row `index`, scrolling it into view.
    fn move_to(&mut self, index: usize, area: Rect) {
        if self.rows.is_empty() {
            return;
        }
        self.index = index.min(self.rows.len() - 1);

        let visible = usize::from(self.visible_rows(area));
        // No room for a list at all puts the highlighted row at the top, so
        // that whatever single row does fit is the right one.
        if visible == 0 || self.index < self.offset {
            self.offset = self.index;
        } else if self.index >= self.offset + visible {
            self.offset = self.index + 1 - visible;
        }
    }

    /// Rows of the list that fit on screen at once.
    fn visible_rows(&self, area: Rect) -> u16 {
        self.rect(area).map_or(0, |rect| rect.height - 2)
    }

    /// Where the pop-up goes, or `None` when the terminal is too small for
    /// even a one-row list.
    fn rect(&self, area: Rect) -> Option<Rect> {
        let frame = 2; // one column or row of frame on each side
        let rows = u16::try_from(self.rows.len()).unwrap_or(u16::MAX);
        let width = (self.inner + frame).min(area.width.saturating_sub(MARGIN * 2));
        let height = (rows + frame).min(area.height.saturating_sub(MARGIN * 2));
        if width < 3 || height < 3 {
            return None;
        }
        Some(Rect::new(area.x + MARGIN, area.y + MARGIN, width, height))
    }

    /// Draw the pop-up over whatever is already in the buffer.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        let Some(rect) = self.rect(area) else {
            return;
        };
        // Nearly black behind the panel: a list has to be readable over a
        // fire or a starfield.
        render::fade_to_black(buf, rect, PANEL_KEEP);

        let frame = Style::new().fg(Color::DarkGray);
        let plain = Style::new().fg(Color::Gray);
        let picked = Style::new().fg(Color::Black).bg(Color::Gray);
        let width = usize::from(rect.width);

        buf.set_stringn(rect.x, rect.y, &self.top, width, frame);
        buf.set_stringn(rect.x, rect.bottom() - 1, &self.bottom, width, frame);

        // What the frame leaves, which is less than `self.inner` once the
        // panel has been clipped to a narrow terminal.
        let inner = usize::from(rect.width - 2);
        let last = rect.x + rect.width - 1;
        for (row, line) in self
            .rows
            .iter()
            .enumerate()
            .skip(self.offset)
            .take(usize::from(rect.height - 2))
        {
            let y = rect.y + 1 + u16::try_from(row - self.offset).unwrap_or(0);
            let chosen = row == self.index;
            let style = if chosen { picked } else { plain };

            buf.set_stringn(rect.x, y, "│", 1, frame);
            buf.set_stringn(rect.x + 1, y, line, inner, style);
            buf.set_stringn(last, y, "│", 1, frame);
            if chosen {
                // The marker goes over the row's own padding, so the text
                // never shifts as the highlight moves.
                buf.set_stringn(rect.x + 2, y, "›", 1, style);
            }
        }
    }
}

/// The key reminder along the bottom, nvim style.
///
/// Three versions are built up front, widest first, and whichever fits the
/// terminal is the one drawn. Nothing is lost on a narrow screen: the full
/// list is one `?` away.
#[derive(Debug)]
pub struct KeyBar {
    versions: Vec<String>,
}

impl Default for KeyBar {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyBar {
    /// Build every version of the bar.
    pub fn new() -> Self {
        let mut full = String::new();
        for setting in Setting::ALL {
            let _ = write!(full, "{} {} · ", setting.key(), setting.label());
        }
        full.push_str("? tuşlar · q çık");

        let mut some = String::new();
        for setting in [
            Setting::Animation,
            Setting::Theme,
            Setting::ClockFont,
            Setting::ClockScale,
        ] {
            let _ = write!(some, "{} {} · ", setting.key(), setting.label());
        }
        some.push_str("? tuşlar · q çık");

        Self {
            versions: vec![full, some, "? tuşlar · q çık".to_owned(), "?".to_owned()],
        }
    }

    /// The widest version that fits `width`, or `None` if none does.
    pub fn text(&self, width: u16) -> Option<&str> {
        self.versions
            .iter()
            .map(String::as_str)
            .find(|text| text_cols(text) <= width)
    }

    /// Draw the bar along the bottom row of `area`.
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        let Some(text) = self.text(area.width) else {
            return;
        };
        let width = text_cols(text);
        let x = area.x + (area.width - width) / 2;
        let y = area.bottom() - 1;
        render::fade_to_black(buf, Rect::new(x, y, width, 1), BAR_KEEP);
        buf.set_string(x, y, text, Style::new().fg(Color::Gray));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list for `setting`, as the app would open it.
    fn list(setting: Setting, config: &Config) -> (Vec<Item>, usize) {
        choices(setting, config)
    }

    #[test]
    fn every_setting_has_its_own_key() {
        let mut keys: Vec<char> = Setting::ALL.into_iter().map(Setting::key).collect();
        let count = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), count, "two settings share a key: {keys:?}");
    }

    #[test]
    fn no_setting_takes_a_key_the_app_needs_for_itself() {
        // q and Esc quit, ? opens the help, n and p step through the
        // animations, and j and k move inside a list.
        for reserved in ['q', '?', 'n', 'p', 'j', 'k'] {
            if reserved == 'k' {
                // k opens the position list, which is fine: inside a list the
                // menu keys are handled first, so the two never collide.
                continue;
            }
            assert!(
                Setting::from_key(reserved).is_none(),
                "{reserved} is taken by the app"
            );
        }
    }

    #[test]
    fn a_key_finds_its_setting() {
        for setting in Setting::ALL {
            assert_eq!(Setting::from_key(setting.key()), Some(setting));
        }
        assert_eq!(Setting::from_key('ä'), None);
    }

    #[test]
    fn every_list_opens_on_the_value_in_force() {
        let config = Config::default();
        for setting in Setting::ALL {
            let (items, index) = list(setting, &config);
            assert!(!items.is_empty(), "{setting:?} offers nothing");
            assert!(index < items.len(), "{setting:?} opened out of range");

            // Putting the line it opened on back is a no-op, which is what
            // makes browsing and then pressing Esc safe.
            let mut copy = config.clone();
            apply(setting, &items[index].value, &mut copy);
            assert_eq!(copy, config, "{setting:?} changed on opening");
        }
    }

    #[test]
    fn every_offered_value_is_a_valid_setting() {
        let config = Config::default();
        for setting in Setting::ALL {
            let (items, _) = list(setting, &config);
            for item in &items {
                let mut copy = config.clone();
                apply(setting, &item.value, &mut copy);
                // Round trip through the command line layer, which is where
                // validation lives.
                let printed = copy.to_toml().expect("settings should render");
                let parsed: Config = toml::from_str(&printed).expect("and parse back");
                assert_eq!(parsed, copy, "{setting:?} = {:?}", item.value);
            }
        }
    }

    #[test]
    fn picking_an_animation_pins_it_and_tumu_unpins() {
        let mut config = Config::default();
        let (items, index) = list(Setting::Animation, &config);
        assert_eq!(index, 0, "the default draws from every animation");
        assert_eq!(items[0].value, "", "the first line should be `all of them`");

        apply(Setting::Animation, "fire", &mut config);
        assert_eq!(config.anim, ["fire"]);
        let (_, index) = list(Setting::Animation, &config);
        assert_eq!(items[index].value, "fire", "the list lost the pinned one");

        apply(Setting::Animation, "", &mut config);
        assert!(config.anim.is_empty());
    }

    #[test]
    fn a_hand_written_number_stays_in_the_list() {
        let config = Config {
            fps: 45,
            cycle_minutes: 7,
            ..Config::default()
        };
        let (items, index) = list(Setting::Fps, &config);
        assert_eq!(items[index].value, "45");
        let (items, index) = list(Setting::Cycle, &config);
        assert_eq!(items[index].value, "7");
    }

    #[test]
    fn several_pinned_animations_are_shown_as_all_of_them() {
        let config = Config {
            anim: vec!["fire".to_owned(), "snow".to_owned()],
            ..Config::default()
        };
        let (_, index) = list(Setting::Animation, &config);
        assert_eq!(index, 0, "two pinned animations are not a line on offer");
    }

    /// A menu for `setting` on an 80x24 screen.
    fn menu(setting: Setting) -> Menu {
        let config = Config::default();
        let (items, index) = choices(setting, &config);
        Menu::new(setting, &items, index, Rect::new(0, 0, 80, 24))
    }

    #[test]
    fn moving_wraps_at_both_ends() {
        let area = Rect::new(0, 0, 80, 24);
        let mut list = menu(Setting::Theme);
        let count = list.rows.len();

        list.move_to(0, area);
        list.move_by(-1, area);
        assert_eq!(list.index, count - 1, "up from the first row wraps round");
        list.move_by(1, area);
        assert_eq!(list.index, 0, "and back down again");
    }

    #[test]
    fn the_row_it_opened_on_is_remembered() {
        let config = Config {
            theme: "nord".to_owned(),
            ..Config::default()
        };
        let (items, index) = choices(Setting::Theme, &config);
        let mut list = Menu::new(Setting::Theme, &items, index, Rect::new(0, 0, 80, 24));
        assert_eq!(list.selected_value(), Some("nord"));

        list.move_by(1, Rect::new(0, 0, 80, 24));
        assert_ne!(list.selected_value(), Some("nord"), "nothing moved");
        assert_eq!(list.opened_value(), Some("nord"), "Esc would go nowhere");
    }

    #[test]
    fn a_list_too_tall_for_the_screen_scrolls() {
        let area = Rect::new(0, 0, 40, 8); // 8 rows: 4 for the list itself
        let config = Config::default();
        let (items, index) = choices(Setting::Animation, &config);
        let mut list = Menu::new(Setting::Animation, &items, index, area);
        assert!(items.len() > 4, "this test needs a list that cannot fit");
        assert_eq!(list.offset, 0);

        for _ in 0..items.len() - 1 {
            list.move_by(1, area);
        }
        assert_eq!(list.index, items.len() - 1);
        assert!(
            list.index < list.offset + usize::from(list.visible_rows(area)),
            "the highlighted row scrolled off screen"
        );
    }

    #[test]
    fn the_help_list_changes_nothing() {
        let list = Menu::help(Rect::new(0, 0, 80, 24));
        assert_eq!(list.setting(), None);
        assert_eq!(list.selected_value(), None);
        assert_eq!(list.opened_value(), None);
    }

    #[test]
    fn the_help_list_mentions_every_key() {
        let list = Menu::help(Rect::new(0, 0, 80, 40));
        let text = list.rows.join("\n");
        for setting in Setting::ALL {
            assert!(
                text.contains(setting.title()),
                "{} is missing from the help",
                setting.title()
            );
        }
    }

    #[test]
    fn rows_are_all_one_width() {
        for setting in Setting::ALL {
            let list = menu(setting);
            for row in &list.rows {
                assert_eq!(
                    text_cols(row),
                    list.inner,
                    "{:?}: {row:?} is not {} columns",
                    setting,
                    list.inner
                );
            }
            assert_eq!(text_cols(&list.top), list.inner + 2);
            assert_eq!(text_cols(&list.bottom), list.inner + 2);
        }
    }

    #[test]
    fn a_screen_too_small_for_a_panel_gets_none() {
        let list = menu(Setting::Theme);
        for (width, height) in [(0, 0), (1, 1), (4, 4), (6, 2), (2, 6)] {
            let area = Rect::new(0, 0, width, height);
            assert!(
                list.rect(area).is_none(),
                "{width}x{height} should be too small"
            );
        }
        assert!(list.rect(Rect::new(0, 0, 20, 6)).is_some());
    }

    #[test]
    fn the_panel_never_reaches_outside_the_screen() {
        let list = menu(Setting::Animation);
        for (width, height) in [(20, 6), (40, 10), (80, 24), (300, 100)] {
            let area = Rect::new(0, 0, width, height);
            let rect = list.rect(area).expect("this should fit");
            assert!(rect.right() <= area.right(), "{width}x{height}");
            assert!(rect.bottom() <= area.bottom(), "{width}x{height}");
        }
    }

    #[test]
    fn a_clipped_panel_stays_inside_its_own_frame() {
        let list = menu(Setting::Cycle); // the longest heading of the lot
        let area = Rect::new(0, 0, 18, 8);
        let rect = list.rect(area).expect("a panel should still fit");
        assert!(
            rect.width < list.inner + 2,
            "this test needs a panel too wide for the screen"
        );

        // Mark every cell, so anything the panel writes stands out.
        let mut buf = Buffer::empty(area);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_char('#');
            }
        }
        list.render(area, &mut buf);

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if rect.contains((x, y).into()) {
                    continue;
                }
                assert_eq!(
                    buf[(x, y)].symbol(),
                    "#",
                    "the panel wrote outside itself at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn the_key_bar_gives_up_a_version_at_a_time() {
        let bar = KeyBar::new();
        let wide = bar.text(200).expect("a wide terminal fits everything");
        assert!(wide.contains("animasyon") && wide.contains("hız"));

        let narrow = bar.text(60).expect("something should still fit 60");
        assert!(text_cols(narrow) <= 60);
        assert!(narrow.contains("q çık"), "quitting should always be shown");

        let tiny = bar.text(16).expect("something should still fit 16");
        assert!(text_cols(tiny) <= 16);
        assert_eq!(bar.text(0), None, "nothing fits nothing");
    }

    #[test]
    fn every_key_bar_version_fits_the_one_before_it() {
        let bar = KeyBar::new();
        for pair in bar.versions.windows(2) {
            assert!(
                text_cols(&pair[1]) < text_cols(&pair[0]),
                "{:?} is not narrower than {:?}",
                pair[1],
                pair[0]
            );
        }
    }
}
