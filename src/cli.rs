//! Command line interface definition.
//!
//! Every setting that also lives in the config file is an `Option` here, so
//! that "not given" can be told apart from "given this value". The merging
//! itself is [`crate::config::Config`]'s job.

use clap::Parser;

use crate::{clock::HourFormat, color::ColorSetting, config::MAX_FPS};

/// An animated, clock-centered terminal screensaver.
#[derive(Debug, Parser)]
#[command(name = "binkam", version, about)]
pub struct Cli {
    /// Target frames per second for the render loop.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..=i64::from(MAX_FPS)))]
    pub fps: Option<u16>,

    /// Background animation to use; repeat to cycle through several.
    ///
    /// Left out, every animation is used.
    #[arg(
        long,
        value_parser = clap::builder::PossibleValuesParser::new(crate::anim::all_names()),
    )]
    pub anim: Vec<String>,

    /// Minutes between automatic animation changes; 0 turns it off.
    #[arg(long)]
    pub cycle_minutes: Option<u64>,

    /// Colour theme for the clock and the animations.
    #[arg(
        long,
        value_parser = clap::builder::PossibleValuesParser::new(crate::theme::Theme::all_names()),
    )]
    pub theme: Option<String>,

    /// Where the clock sits on screen, and whether it moves.
    #[arg(
        long,
        value_parser = clap::builder::PossibleValuesParser::new(
            crate::clock::motion::ClockPosition::all_names(),
        ),
    )]
    pub clock_position: Option<String>,

    /// Show the seconds as part of the time.
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub show_seconds: Option<bool>,

    /// Show the date underneath the clock.
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub show_date: Option<bool>,

    /// Hours on the dial: 24 for 00-23, 12 for 01-12.
    #[arg(long, value_name = "HOURS", value_parser = ["12", "24"])]
    pub hour_format: Option<String>,

    /// Darken the animation behind the clock, so the digits stay legible.
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub backdrop: Option<bool>,

    /// Shorthand for `--backdrop false`.
    #[arg(long, conflicts_with = "backdrop")]
    pub no_backdrop: bool,

    /// How much colour to use.
    ///
    /// `auto` works it out from `COLORTERM`, `TERM` and `NO_COLOR`.
    #[arg(
        long,
        value_name = "MODE",
        value_parser = clap::builder::PossibleValuesParser::new(ColorSetting::all_names()),
    )]
    pub color: Option<String>,

    /// Shorthand for `--color none`.
    #[arg(long, conflicts_with = "color")]
    pub no_color: bool,

    /// Seed the randomness, for a reproducible picture.
    #[arg(long)]
    pub seed: Option<u64>,

    /// Read settings from this file instead of the usual place.
    #[arg(long, value_name = "PATH")]
    pub config: Option<String>,

    /// Print the settings this run would use, as TOML, and exit.
    #[arg(long)]
    pub print_config: bool,

    /// Browse animations and themes instead of quitting on a key press.
    ///
    /// n and p change animation, t the theme, c where the clock sits, and
    /// q or Esc quits.
    #[arg(long)]
    pub preview: bool,

    /// Show a status and frames-per-second readout in the top right corner.
    #[arg(long)]
    pub show_fps: bool,

    /// List the available animations and exit.
    #[arg(long)]
    pub list: bool,

    /// List the available colour themes and exit.
    #[arg(long)]
    pub list_themes: bool,

    /// Time this many frames off screen, report the cost and exit.
    ///
    /// Build with `--release` for a number worth reading. The time covers
    /// updating, drawing and ratatui's own diffing, but not writing to a
    /// real terminal.
    #[arg(long, value_name = "FRAMES", value_parser = clap::value_parser!(u32).range(1..))]
    pub bench: Option<u32>,

    /// Render a single frame to stdout as plain text and exit.
    ///
    /// Uses a headless backend, so the real terminal is never touched.
    #[arg(long)]
    pub dump_frame: bool,

    /// Terminal size used by `--dump-frame`, given as `WIDTHxHEIGHT` (e.g. 80x24).
    #[arg(long, value_parser = parse_size)]
    pub size: Option<(u16, u16)>,
}

impl Cli {
    /// Size to use for a headless frame dump, falling back to a sane default.
    pub fn dump_size(&self) -> (u16, u16) {
        self.size.unwrap_or((80, 24))
    }

    /// Whether to darken behind the clock, or `None` to leave it to the file.
    pub fn backdrop(&self) -> Option<bool> {
        if self.no_backdrop {
            Some(false)
        } else {
            self.backdrop
        }
    }

    /// The colour setting asked for, or `None` to leave it to the file.
    pub fn color(&self) -> Option<ColorSetting> {
        if self.no_color {
            return Some(ColorSetting::None);
        }
        ColorSetting::by_name(self.color.as_deref()?)
    }

    /// The hour format asked for, or `None` to leave it to the file.
    ///
    /// The value parser has already limited this to `12` or `24`.
    pub fn hour_format(&self) -> Option<HourFormat> {
        let hours = self.hour_format.as_deref()?.parse().ok()?;
        HourFormat::from_hours(hours)
    }
}

/// Parse a `WIDTHxHEIGHT` string such as `80x24`.
fn parse_size(raw: &str) -> Result<(u16, u16), String> {
    let (w, h) = raw
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("expected WIDTHxHEIGHT (e.g. 80x24), got `{raw}`"))?;

    let parse = |part: &str, what: &str| -> Result<u16, String> {
        part.trim()
            .parse::<u16>()
            .map_err(|e| format!("invalid {what} `{part}`: {e}"))
            .and_then(|v| {
                if v == 0 {
                    Err(format!("{what} must be greater than zero"))
                } else {
                    Ok(v)
                }
            })
    };

    Ok((parse(w, "width")?, parse(h, "height")?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli(args: &[&str]) -> Cli {
        let mut full = vec!["binkam"];
        full.extend_from_slice(args);
        Cli::parse_from(full)
    }

    #[test]
    fn parses_valid_sizes() {
        assert_eq!(parse_size("80x24").unwrap(), (80, 24));
        assert_eq!(parse_size("200X50").unwrap(), (200, 50));
    }

    #[test]
    fn rejects_malformed_sizes() {
        for bad in ["80", "80x", "x24", "0x24", "80x0", "axb", "-1x10"] {
            assert!(parse_size(bad).is_err(), "`{bad}` should not parse");
        }
    }

    #[test]
    fn dump_size_defaults_to_80x24() {
        assert_eq!(cli(&[]).dump_size(), (80, 24));
        assert_eq!(cli(&["--size", "40x10"]).dump_size(), (40, 10));
    }

    #[test]
    fn nothing_is_set_without_arguments() {
        let cli = cli(&[]);
        assert_eq!(cli.fps, None);
        assert_eq!(cli.theme, None);
        assert_eq!(cli.clock_position, None);
        assert_eq!(cli.cycle_minutes, None);
        assert_eq!(cli.seed, None);
        assert_eq!(cli.show_seconds, None);
        assert_eq!(cli.show_date, None);
        assert_eq!(cli.hour_format(), None);
        assert_eq!(cli.backdrop(), None);
        assert_eq!(cli.color(), None);
        assert!(cli.anim.is_empty());
    }

    #[test]
    fn anim_can_be_repeated() {
        assert_eq!(cli(&["--anim", "fire"]).anim, ["fire"]);
        assert_eq!(
            cli(&["--anim", "fire", "--anim", "snow"]).anim,
            ["fire", "snow"]
        );
    }

    #[test]
    fn unknown_names_are_rejected_up_front() {
        for bad in [
            vec!["--anim", "nope"],
            vec!["--theme", "nope"],
            vec!["--clock-position", "nope"],
            vec!["--hour-format", "13"],
            vec!["--color", "16"],
            vec!["--bench", "0"],
            vec!["--fps", "0"],
            vec!["--fps", "9999"],
        ] {
            let mut full = vec!["binkam"];
            full.extend_from_slice(&bad);
            assert!(Cli::try_parse_from(full).is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn backdrop_has_three_states() {
        assert_eq!(cli(&[]).backdrop(), None);
        assert_eq!(cli(&["--backdrop"]).backdrop(), Some(true));
        assert_eq!(cli(&["--backdrop", "false"]).backdrop(), Some(false));
        assert_eq!(cli(&["--no-backdrop"]).backdrop(), Some(false));
        assert!(Cli::try_parse_from(["binkam", "--backdrop", "--no-backdrop"]).is_err());
    }

    #[test]
    fn hour_format_accepts_only_twelve_or_twenty_four() {
        assert_eq!(
            cli(&["--hour-format", "12"]).hour_format(),
            Some(HourFormat::H12)
        );
        assert_eq!(
            cli(&["--hour-format", "24"]).hour_format(),
            Some(HourFormat::H24)
        );
    }

    #[test]
    fn colour_has_a_shorthand_for_none() {
        assert_eq!(cli(&["--no-color"]).color(), Some(ColorSetting::None));
        assert_eq!(
            cli(&["--color", "truecolor"]).color(),
            Some(ColorSetting::Truecolor)
        );
        assert!(Cli::try_parse_from(["binkam", "--color", "256", "--no-color"]).is_err());
    }

    #[test]
    fn verifies_cli_definition() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
