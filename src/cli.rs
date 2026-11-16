//! Command line interface definition.

use clap::Parser;

use crate::clock::HourFormat;

/// An animated, clock-centered terminal screensaver.
#[derive(Debug, Parser)]
#[command(name = "kumsaati", version, about)]
pub struct Cli {
    /// Target frames per second for the render loop.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u16).range(1..=240))]
    pub fps: u16,

    /// Render a single frame to stdout as plain text and exit.
    ///
    /// Uses a headless backend, so the real terminal is never touched.
    #[arg(long)]
    pub dump_frame: bool,

    /// Terminal size used by `--dump-frame`, given as WIDTHxHEIGHT (e.g. 80x24).
    #[arg(long, value_parser = parse_size)]
    pub size: Option<(u16, u16)>,

    /// Background animation to run.
    ///
    /// Left out, the screensaver starts on a random one and keeps changing.
    #[arg(
        long,
        value_parser = clap::builder::PossibleValuesParser::new(crate::anim::all_names()),
    )]
    pub anim: Option<String>,

    /// Minutes between automatic animation changes; 0 turns it off.
    ///
    /// Defaults to off when --anim pins one animation, and to a few minutes
    /// otherwise.
    #[arg(long)]
    pub cycle_minutes: Option<u64>,

    /// List the available animations and exit.
    #[arg(long)]
    pub list: bool,

    /// List the available colour themes and exit.
    #[arg(long)]
    pub list_themes: bool,

    /// Seed the animation's randomness, for a reproducible picture.
    #[arg(long)]
    pub seed: Option<u64>,

    /// Show a frames-per-second counter in the top right corner.
    #[arg(long)]
    pub show_fps: bool,

    /// Darken the animation behind the clock, so the digits stay legible.
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub backdrop: Option<bool>,

    /// Shorthand for `--backdrop false`.
    #[arg(long, conflicts_with = "backdrop")]
    pub no_backdrop: bool,

    /// Show the seconds as part of the time.
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub show_seconds: Option<bool>,

    /// Show the date underneath the clock.
    #[arg(long, num_args = 0..=1, default_missing_value = "true", value_name = "BOOL")]
    pub show_date: Option<bool>,

    /// Hours on the dial: 24 for 00-23, 12 for 01-12.
    #[arg(long, value_name = "HOURS", value_parser = ["12", "24"])]
    pub hour_format: Option<String>,

    /// Where the clock sits on screen, and whether it moves.
    #[arg(
        long,
        default_value = "center",
        value_parser = clap::builder::PossibleValuesParser::new(
            crate::clock::motion::ClockPosition::all_names(),
        ),
    )]
    pub clock_position: String,

    /// Browse animations and themes instead of quitting on a key press.
    ///
    /// n and p change animation, t the theme, c where the clock sits, and
    /// q or Esc quits.
    #[arg(long)]
    pub preview: bool,

    /// Colour theme for the clock and the animations.
    #[arg(
        long,
        default_value = crate::theme::DEFAULT,
        value_parser = clap::builder::PossibleValuesParser::new(crate::theme::Theme::all_names()),
    )]
    pub theme: String,
}

/// Minutes between animation changes when nothing says otherwise.
const DEFAULT_CYCLE_MINUTES: u64 = 3;

impl Cli {
    /// Size to use for a headless frame dump, falling back to a sane default.
    pub fn dump_size(&self) -> (u16, u16) {
        self.size.unwrap_or((80, 24))
    }

    /// Duration of a single frame derived from [`Cli::fps`].
    pub fn frame_time(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(1.0 / f64::from(self.fps))
    }

    /// Whether to darken behind the clock, or `None` to leave it to the
    /// config file.
    pub fn backdrop(&self) -> Option<bool> {
        if self.no_backdrop {
            Some(false)
        } else {
            self.backdrop
        }
    }

    /// The hour format asked for, or `None` to leave it to the config file.
    ///
    /// The value parser has already limited this to `12` or `24`.
    pub fn hour_format(&self) -> Option<HourFormat> {
        let hours = self.hour_format.as_deref()?.parse().ok()?;
        HourFormat::from_hours(hours)
    }

    /// How long to wait between animation changes, or `None` for never.
    pub fn cycle_interval(&self) -> Option<std::time::Duration> {
        let minutes = match self.cycle_minutes {
            Some(minutes) => minutes,
            // Pinning one animation turns cycling off, and so does preview
            // mode, where changing animation is the user's job.
            None if self.anim.is_some() || self.preview => 0,
            None => DEFAULT_CYCLE_MINUTES,
        };
        (minutes > 0).then(|| std::time::Duration::from_secs(minutes * 60))
    }

    /// The seed to hand animations: the one given, or a fresh one per run.
    pub fn resolved_seed(&self) -> u64 {
        self.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0x5EED_5EED, |since| since.as_nanos() as u64)
        })
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
    fn frame_time_matches_fps() {
        let cli = Cli::parse_from(["kumsaati", "--fps", "50"]);
        assert_eq!(cli.frame_time(), std::time::Duration::from_millis(20));
    }

    #[test]
    fn dump_size_defaults_to_80x24() {
        let cli = Cli::parse_from(["kumsaati"]);
        assert_eq!(cli.dump_size(), (80, 24));
    }

    #[test]
    fn no_animation_is_pinned_by_default() {
        let cli = Cli::parse_from(["kumsaati"]);
        assert_eq!(cli.anim, None);
    }

    #[test]
    fn cycling_is_on_by_default_and_off_once_an_animation_is_pinned() {
        let roaming = Cli::parse_from(["kumsaati"]);
        assert_eq!(
            roaming.cycle_interval(),
            Some(std::time::Duration::from_secs(DEFAULT_CYCLE_MINUTES * 60))
        );

        let pinned = Cli::parse_from(["kumsaati", "--anim", "fire"]);
        assert_eq!(pinned.cycle_interval(), None);
    }

    #[test]
    fn backdrop_has_three_states() {
        assert_eq!(Cli::parse_from(["kumsaati"]).backdrop(), None);
        assert_eq!(
            Cli::parse_from(["kumsaati", "--backdrop"]).backdrop(),
            Some(true)
        );
        assert_eq!(
            Cli::parse_from(["kumsaati", "--backdrop", "false"]).backdrop(),
            Some(false)
        );
        assert_eq!(
            Cli::parse_from(["kumsaati", "--no-backdrop"]).backdrop(),
            Some(false)
        );
        assert!(Cli::try_parse_from(["kumsaati", "--backdrop", "--no-backdrop"]).is_err());
    }

    #[test]
    fn hour_format_accepts_only_twelve_or_twenty_four() {
        assert_eq!(Cli::parse_from(["kumsaati"]).hour_format(), None);
        assert_eq!(
            Cli::parse_from(["kumsaati", "--hour-format", "12"]).hour_format(),
            Some(HourFormat::H12)
        );
        assert_eq!(
            Cli::parse_from(["kumsaati", "--hour-format", "24"]).hour_format(),
            Some(HourFormat::H24)
        );
        assert!(Cli::try_parse_from(["kumsaati", "--hour-format", "13"]).is_err());
    }

    #[test]
    fn preview_mode_does_not_cycle_on_its_own() {
        let cli = Cli::parse_from(["kumsaati", "--preview"]);
        assert_eq!(cli.cycle_interval(), None);
    }

    #[test]
    fn an_explicit_interval_wins_either_way() {
        let forced = Cli::parse_from(["kumsaati", "--anim", "fire", "--cycle-minutes", "5"]);
        assert_eq!(
            forced.cycle_interval(),
            Some(std::time::Duration::from_secs(300))
        );

        let disabled = Cli::parse_from(["kumsaati", "--cycle-minutes", "0"]);
        assert_eq!(disabled.cycle_interval(), None);
    }

    #[test]
    fn defaults_to_a_known_theme() {
        let cli = Cli::parse_from(["kumsaati"]);
        assert_eq!(cli.theme, crate::theme::DEFAULT);
        assert!(crate::theme::Theme::by_name(&cli.theme).is_some());
    }

    #[test]
    fn defaults_to_a_known_clock_position() {
        use crate::clock::motion::ClockPosition;
        let cli = Cli::parse_from(["kumsaati"]);
        assert_eq!(cli.clock_position, "center");
        assert!(ClockPosition::by_name(&cli.clock_position).is_some());
    }

    #[test]
    fn rejects_unknown_clock_positions() {
        assert!(Cli::try_parse_from(["kumsaati", "--clock-position", "nope"]).is_err());
    }

    #[test]
    fn rejects_unknown_themes() {
        assert!(Cli::try_parse_from(["kumsaati", "--theme", "nope"]).is_err());
    }

    #[test]
    fn rejects_unknown_animations() {
        assert!(Cli::try_parse_from(["kumsaati", "--anim", "nope"]).is_err());
    }

    #[test]
    fn an_explicit_seed_is_stable_across_calls() {
        let cli = Cli::parse_from(["kumsaati", "--seed", "7"]);
        assert_eq!(cli.resolved_seed(), 7);
        assert_eq!(cli.resolved_seed(), 7);
    }

    #[test]
    fn verifies_cli_definition() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
