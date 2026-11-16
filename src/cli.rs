//! Command line interface definition.

use clap::Parser;

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
    #[arg(
        long,
        default_value = DEFAULT_ANIM,
        value_parser = clap::builder::PossibleValuesParser::new(crate::anim::all_names()),
    )]
    pub anim: String,

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

    /// Do not darken the animation behind the clock.
    #[arg(long)]
    pub no_backdrop: bool,

    /// Colour theme for the clock and the animations.
    #[arg(
        long,
        default_value = crate::theme::DEFAULT,
        value_parser = clap::builder::PossibleValuesParser::new(crate::theme::Theme::all_names()),
    )]
    pub theme: String,
}

/// Animation used when `--anim` is not given.
const DEFAULT_ANIM: &str = "starfield";

impl Cli {
    /// Size to use for a headless frame dump, falling back to a sane default.
    pub fn dump_size(&self) -> (u16, u16) {
        self.size.unwrap_or((80, 24))
    }

    /// Duration of a single frame derived from [`Cli::fps`].
    pub fn frame_time(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(1.0 / f64::from(self.fps))
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
    fn defaults_to_a_known_animation() {
        let cli = Cli::parse_from(["kumsaati"]);
        assert_eq!(cli.anim, DEFAULT_ANIM);
        assert!(crate::anim::all_names().contains(&DEFAULT_ANIM));
    }

    #[test]
    fn defaults_to_a_known_theme() {
        let cli = Cli::parse_from(["kumsaati"]);
        assert_eq!(cli.theme, crate::theme::DEFAULT);
        assert!(crate::theme::Theme::by_name(&cli.theme).is_some());
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
