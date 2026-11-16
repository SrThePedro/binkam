//! The settings for a run, and where they come from.
//!
//! Three layers, each beating the one before it: the built-in defaults, then
//! the config file, then the command line. The file is optional; without one
//! the defaults are the settings.

use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use color_eyre::eyre::{Result, WrapErr, bail};
use directories::ProjectDirs;
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    anim,
    cli::Cli,
    clock::{HourFormat, motion::ClockPosition},
    color::ColorSetting,
    theme::{self, Theme},
};

/// Name of the file inside the config directory.
const FILE_NAME: &str = "config.toml";

/// Application name, which decides the config directory.
const APP_NAME: &str = "kumsaati";

/// Highest frame rate that still makes sense to ask for.
///
/// The command line parser uses this too, so both layers agree.
pub const MAX_FPS: u16 = 240;

/// Everything the screensaver needs to know, with every layer merged in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    /// Frames drawn per second.
    pub fps: u16,
    /// Animations to use. Empty means all of them; one means just that one.
    ///
    /// In the file this takes either a string or a list of strings.
    #[serde(deserialize_with = "animation_list")]
    pub anim: Vec<String>,
    /// Minutes between automatic animation changes; 0 turns it off.
    pub cycle_minutes: u64,
    /// Colour theme name.
    pub theme: String,
    /// Clock position mode name.
    pub clock_position: String,
    /// Whether the time carries its seconds.
    pub show_seconds: bool,
    /// Whether the date line is drawn.
    pub show_date: bool,
    /// Hours on the dial: 12 or 24.
    pub hour_format: u8,
    /// Whether to darken the animation behind the clock.
    pub clock_backdrop: bool,
    /// How much colour to use: `auto`, `truecolor`, `256` or `none`.
    pub color: String,
    /// Fixed randomness seed, or `None` for a different one every run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fps: 30,
            anim: Vec::new(),
            cycle_minutes: 3,
            theme: theme::DEFAULT.to_owned(),
            clock_position: "center".to_owned(),
            show_seconds: true,
            show_date: true,
            hour_format: HourFormat::H24.hours(),
            clock_backdrop: true,
            color: ColorSetting::default().name().to_owned(),
            seed: None,
        }
    }
}

impl Config {
    /// Where the config file lives on this platform.
    ///
    /// `~/.config/kumsaati/config.toml` on Linux, inside `%APPDATA%` on
    /// Windows. `None` if the platform has no such notion of a home.
    pub fn default_path() -> Option<PathBuf> {
        ProjectDirs::from("", "", APP_NAME).map(|dirs| dirs.config_dir().join(FILE_NAME))
    }

    /// Read and parse one config file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .wrap_err_with(|| format!("could not read {}", path.display()))?;
        // A toml parse error already says which line and column went wrong.
        toml::from_str(&text).wrap_err_with(|| format!("could not parse {}", path.display()))
    }

    /// The settings for this run: the file under the command line.
    pub fn resolve(cli: &Cli) -> Result<Self> {
        Self::merge(Self::from_layers(cli)?, cli)
    }

    /// The file layer: whichever file applies, or the defaults.
    fn from_layers(cli: &Cli) -> Result<Self> {
        match cli.config.as_deref() {
            // A file named on the command line has to be there.
            Some(path) => Self::from_file(Path::new(path)),
            None => match Self::default_path() {
                Some(path) if path.is_file() => Self::from_file(&path),
                // No file at all is perfectly normal.
                _ => Ok(Self::default()),
            },
        }
    }

    /// Lay the command line over `base` and check the result makes sense.
    pub fn merge(mut base: Self, cli: &Cli) -> Result<Self> {
        if let Some(fps) = cli.fps {
            base.fps = fps;
        }
        if !cli.anim.is_empty() {
            base.anim = cli.anim.clone();
        }
        if let Some(minutes) = cli.cycle_minutes {
            base.cycle_minutes = minutes;
        }
        if let Some(theme) = &cli.theme {
            base.theme = theme.clone();
        }
        if let Some(position) = &cli.clock_position {
            base.clock_position = position.clone();
        }
        if let Some(show) = cli.show_seconds {
            base.show_seconds = show;
        }
        if let Some(show) = cli.show_date {
            base.show_date = show;
        }
        if let Some(format) = cli.hour_format() {
            base.hour_format = format.hours();
        }
        if let Some(backdrop) = cli.backdrop() {
            base.clock_backdrop = backdrop;
        }
        if let Some(color) = cli.color() {
            base.color = color.name().to_owned();
        }
        if let Some(seed) = cli.seed {
            base.seed = Some(seed);
        }

        base.validate()?;
        Ok(base)
    }

    /// Reject settings that no amount of later code could make sense of.
    fn validate(&self) -> Result<()> {
        if self.fps == 0 || self.fps > MAX_FPS {
            bail!("fps must be between 1 and {MAX_FPS}, got {}", self.fps);
        }
        for name in &self.anim {
            if !anim::all_names().contains(&name.as_str()) {
                bail!(
                    "unknown animation `{name}`; known animations are {}",
                    anim::all_names().join(", ")
                );
            }
        }
        if Theme::by_name(&self.theme).is_none() {
            bail!(
                "unknown theme `{}`; known themes are {}",
                self.theme,
                Theme::all_names().join(", ")
            );
        }
        if ClockPosition::by_name(&self.clock_position).is_none() {
            bail!(
                "unknown clock_position `{}`; known positions are {}",
                self.clock_position,
                ClockPosition::all_names().join(", ")
            );
        }
        if HourFormat::from_hours(self.hour_format).is_none() {
            bail!("hour_format must be 12 or 24, got {}", self.hour_format);
        }
        if ColorSetting::by_name(&self.color).is_none() {
            bail!(
                "unknown color `{}`; known settings are {}",
                self.color,
                ColorSetting::all_names().join(", ")
            );
        }
        Ok(())
    }

    /// How long one frame should take.
    pub fn frame_time(&self) -> Duration {
        Duration::from_secs_f64(1.0 / f64::from(self.fps))
    }

    /// The seed to start from: the one given, or a fresh one per run.
    pub fn resolved_seed(&self) -> u64 {
        self.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0x5EED_5EED, |since| since.as_nanos() as u64)
        })
    }

    /// The hour format, already known to be valid.
    pub fn hour_format(&self) -> HourFormat {
        HourFormat::from_hours(self.hour_format).unwrap_or_default()
    }

    /// The colour setting, already known to be valid.
    pub fn color_setting(&self) -> ColorSetting {
        ColorSetting::by_name(&self.color).unwrap_or_default()
    }

    /// The clock position, already known to be valid.
    pub fn clock_position(&self) -> ClockPosition {
        ClockPosition::by_name(&self.clock_position).unwrap_or(ClockPosition::Center)
    }

    /// The animations to draw from, in registry order.
    ///
    /// Empty settings mean every animation. Names were checked by
    /// [`Config::validate`], so this hands back the `'static` registry
    /// entries rather than the owned strings.
    pub fn animation_pool(&self) -> Vec<&'static str> {
        if self.anim.is_empty() {
            return anim::all_names().to_vec();
        }
        self.anim
            .iter()
            .filter_map(|wanted| {
                anim::all_names()
                    .iter()
                    .copied()
                    .find(|known| known == wanted)
            })
            .collect()
    }

    /// How long to wait between animation changes, or `None` for never.
    ///
    /// With a single animation to draw from there is nothing to change to.
    pub fn cycle_interval(&self) -> Option<Duration> {
        if self.cycle_minutes == 0 || self.animation_pool().len() < 2 {
            return None;
        }
        Some(Duration::from_secs(self.cycle_minutes * 60))
    }

    /// The settings as TOML, for `--print-config`.
    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).wrap_err("could not render the settings as TOML")
    }
}

/// Accept either one animation name or a list of them.
fn animation_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    // An untagged enum tries each variant in turn, which is exactly the
    // "a string or a list of strings" rule written down.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }

    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(name) => vec![name],
        OneOrMany::Many(names) => names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn cli(args: &[&str]) -> Cli {
        let mut full = vec!["kumsaati"];
        full.extend_from_slice(args);
        Cli::parse_from(full)
    }

    fn merged(file: Config, args: &[&str]) -> Config {
        Config::merge(file, &cli(args)).expect("these settings should be valid")
    }

    #[test]
    fn the_defaults_are_valid() {
        Config::default().validate().unwrap();
    }

    #[test]
    fn an_empty_file_leaves_the_defaults_alone() {
        let parsed: Config = toml::from_str("").unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn the_file_beats_the_defaults() {
        let parsed: Config = toml::from_str(
            r#"
            fps = 60
            theme = "nord"
            clock_position = "drift"
            show_seconds = false
            hour_format = 12
            cycle_minutes = 7
            clock_backdrop = false
            seed = 99
            "#,
        )
        .unwrap();

        assert_eq!(parsed.fps, 60);
        assert_eq!(parsed.theme, "nord");
        assert_eq!(parsed.clock_position, "drift");
        assert!(!parsed.show_seconds);
        assert!(parsed.show_date, "untouched keys keep their default");
        assert_eq!(parsed.hour_format, 12);
        assert_eq!(parsed.cycle_minutes, 7);
        assert!(!parsed.clock_backdrop);
        assert_eq!(parsed.seed, Some(99));
        parsed.validate().unwrap();
    }

    #[test]
    fn the_command_line_beats_the_file() {
        let file = Config {
            fps: 60,
            theme: "nord".to_owned(),
            clock_position: "drift".to_owned(),
            show_seconds: false,
            hour_format: 12,
            cycle_minutes: 7,
            clock_backdrop: false,
            seed: Some(99),
            ..Config::default()
        };

        let config = merged(
            file,
            &[
                "--fps",
                "15",
                "--theme",
                "amber",
                "--clock-position",
                "corner",
                "--show-seconds",
                "true",
                "--hour-format",
                "24",
                "--cycle-minutes",
                "1",
                "--backdrop",
                "--seed",
                "5",
            ],
        );

        assert_eq!(config.fps, 15);
        assert_eq!(config.theme, "amber");
        assert_eq!(config.clock_position, "corner");
        assert!(config.show_seconds);
        assert_eq!(config.hour_format, 24);
        assert_eq!(config.cycle_minutes, 1);
        assert!(config.clock_backdrop);
        assert_eq!(config.seed, Some(5));
    }

    #[test]
    fn an_absent_flag_leaves_the_file_in_charge() {
        let file = Config {
            fps: 60,
            theme: "nord".to_owned(),
            show_seconds: false,
            clock_backdrop: false,
            ..Config::default()
        };

        let config = merged(file.clone(), &[]);
        assert_eq!(config, file, "nothing on the command line, nothing changed");
    }

    #[test]
    fn no_backdrop_overrides_a_file_that_wanted_one() {
        let file = Config {
            clock_backdrop: true,
            ..Config::default()
        };
        assert!(!merged(file, &["--no-backdrop"]).clock_backdrop);
    }

    #[test]
    fn anim_takes_one_name_or_a_list() {
        let one: Config = toml::from_str(r#"anim = "fire""#).unwrap();
        assert_eq!(one.anim, ["fire"]);

        let many: Config = toml::from_str(r#"anim = ["fire", "plasma"]"#).unwrap();
        assert_eq!(many.anim, ["fire", "plasma"]);

        let none: Config = toml::from_str("").unwrap();
        assert!(none.anim.is_empty());
    }

    #[test]
    fn repeated_anim_flags_replace_the_file_list() {
        let file = Config {
            anim: vec!["fire".to_owned()],
            ..Config::default()
        };
        let config = merged(file, &["--anim", "snow", "--anim", "rain"]);
        assert_eq!(config.anim, ["snow", "rain"]);
    }

    #[test]
    fn the_animation_pool_falls_back_to_everything() {
        assert_eq!(Config::default().animation_pool(), anim::all_names());

        let picked = Config {
            anim: vec!["plasma".to_owned(), "fire".to_owned()],
            ..Config::default()
        };
        assert_eq!(picked.animation_pool(), ["plasma", "fire"]);
    }

    #[test]
    fn cycling_needs_at_least_two_animations() {
        let every = Config::default();
        assert_eq!(every.cycle_interval(), Some(Duration::from_secs(180)));

        let pinned = Config {
            anim: vec!["fire".to_owned()],
            ..Config::default()
        };
        assert_eq!(pinned.cycle_interval(), None, "nothing to change to");

        let pair = Config {
            anim: vec!["fire".to_owned(), "snow".to_owned()],
            ..Config::default()
        };
        assert_eq!(pair.cycle_interval(), Some(Duration::from_secs(180)));

        let off = Config {
            cycle_minutes: 0,
            ..Config::default()
        };
        assert_eq!(off.cycle_interval(), None);
    }

    #[test]
    fn a_broken_file_says_which_line_is_wrong() {
        let text = "fps = 30\nshow_date = true\ntheme = nord\n";
        let error = toml::from_str::<Config>(text).unwrap_err().to_string();
        assert!(error.contains("line 3"), "no line number in: {error}");
    }

    #[test]
    fn an_unknown_key_is_reported_rather_than_ignored() {
        let error = toml::from_str::<Config>("colour = \"red\"")
            .unwrap_err()
            .to_string();
        assert!(error.contains("colour"), "{error}");
    }

    #[test]
    fn a_missing_file_is_an_error_with_its_path() {
        let path = Path::new("/definitely/not/here/kumsaati.toml");
        let error = format!("{:#}", Config::from_file(path).unwrap_err());
        assert!(error.contains("/definitely/not/here"), "{error}");
    }

    #[test]
    fn bad_values_are_rejected_with_a_readable_message() {
        let cases: Vec<(Config, &str)> = vec![
            (
                Config {
                    fps: 0,
                    ..Config::default()
                },
                "fps",
            ),
            (
                Config {
                    anim: vec!["nope".to_owned()],
                    ..Config::default()
                },
                "unknown animation",
            ),
            (
                Config {
                    theme: "nope".to_owned(),
                    ..Config::default()
                },
                "unknown theme",
            ),
            (
                Config {
                    clock_position: "nope".to_owned(),
                    ..Config::default()
                },
                "unknown clock_position",
            ),
            (
                Config {
                    hour_format: 13,
                    ..Config::default()
                },
                "hour_format",
            ),
            (
                Config {
                    color: "16".to_owned(),
                    ..Config::default()
                },
                "unknown color",
            ),
        ];

        for (config, expected) in cases {
            let error = config.validate().unwrap_err().to_string();
            assert!(
                error.contains(expected),
                "{error} should mention {expected}"
            );
        }
    }

    #[test]
    fn printed_settings_parse_back_to_the_same_thing() {
        let config = Config {
            fps: 45,
            anim: vec!["fire".to_owned(), "snow".to_owned()],
            theme: "gruvbox".to_owned(),
            clock_position: "bounce".to_owned(),
            show_seconds: false,
            hour_format: 12,
            seed: Some(7),
            ..Config::default()
        };

        let text = config.to_toml().unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(
            parsed, config,
            "round trip through TOML changed it:\n{text}"
        );
    }

    #[test]
    fn printed_settings_leave_out_an_unset_seed() {
        let text = Config::default().to_toml().unwrap();
        assert!(!text.contains("seed"), "{text}");
    }

    #[test]
    fn the_colour_setting_comes_from_either_layer() {
        let from_file: Config = toml::from_str(r#"color = "256""#).unwrap();
        assert_eq!(from_file.color_setting(), ColorSetting::Indexed);

        assert_eq!(
            merged(Config::default(), &["--color", "none"]).color_setting(),
            ColorSetting::None
        );
        assert_eq!(
            merged(from_file, &["--no-color"]).color_setting(),
            ColorSetting::None,
            "--no-color has to beat the file"
        );
        assert_eq!(Config::default().color_setting(), ColorSetting::Auto);
    }

    #[test]
    fn frame_time_follows_fps() {
        let config = Config {
            fps: 50,
            ..Config::default()
        };
        assert_eq!(config.frame_time(), Duration::from_millis(20));
    }

    #[test]
    fn an_explicit_seed_is_stable() {
        let config = Config {
            seed: Some(7),
            ..Config::default()
        };
        assert_eq!(config.resolved_seed(), 7);
        assert_eq!(config.resolved_seed(), 7);
    }

    #[test]
    fn the_default_path_is_under_the_app_name() {
        let Some(path) = Config::default_path() else {
            return; // no home directory on this platform
        };
        assert!(
            path.ends_with(Path::new(APP_NAME).join(FILE_NAME)),
            "{path:?}"
        );
    }
}
